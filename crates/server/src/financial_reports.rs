use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, put},
};
use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    AppState,
    auth::AuthUser,
    error::{AppError, AppResult},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/accounting/reports", get(reports))
        .route("/accounting/reports/cash-flow/{id}", put(classify))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ClassificationInput {
    category: Option<String>,
    reason: String,
    version: i64,
    idempotency_key: Uuid,
}

async fn classify(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ClassificationInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    if body
        .category
        .as_deref()
        .is_some_and(|category| !["operating", "investing", "financing"].contains(&category))
        || body.version < 0
    {
        return Err(AppError::bad_request(
            "Invalid cash flow category or version",
        ));
    }
    let reason = body.reason.trim();
    if !(3..=1000).contains(&reason.chars().count()) || reason.chars().any(char::is_control) {
        return Err(AppError::bad_request(
            "Classification reason must contain 3–1000 characters without control characters",
        ));
    }
    let request = crate::business::payload(&format!("cash-flow.classify:{id}"), &user, &body);
    let mut tx = state.db.begin().await?;
    if let Some(value) = crate::business::cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(value));
    }
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("cash-flow:{id}"))
        .execute(&mut *tx)
        .await?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM journal_lines WHERE journal_id=$1 AND account_code='1100')",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if !exists {
        return Err(AppError::not_found());
    }
    let old =
        sqlx::query("SELECT version,category FROM cash_flow_classifications WHERE journal_id=$1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    let version = old.as_ref().map_or(0, |r| r.get::<i64, _>("version"));
    if body.version != version {
        return Err(AppError::conflict(
            "Cash flow classification changed. Reload the report.",
        ));
    }
    let previous: Option<String> = old.as_ref().and_then(|r| r.get("category"));
    let version = version
        .checked_add(1)
        .ok_or_else(|| AppError::conflict("Classification version exhausted"))?;
    sqlx::query("INSERT INTO cash_flow_classifications(journal_id,category,reason,version,updated_by) VALUES($1,$2,$3,$4,$5) ON CONFLICT(journal_id) DO UPDATE SET category=EXCLUDED.category,reason=EXCLUDED.reason,version=EXCLUDED.version,updated_by=EXCLUDED.updated_by,updated_at=now()")
        .bind(id).bind(&body.category).bind(reason).bind(version).bind(user.id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO cash_flow_classification_events(id,journal_id,category,previous_category,reason,version,created_by) VALUES($1,$2,$3,$4,$5,$6,$7)")
        .bind(Uuid::new_v4()).bind(id).bind(&body.category).bind(&previous).bind(reason).bind(version).bind(user.id).execute(&mut *tx).await?;
    let result: Value = sqlx::query_scalar("SELECT jsonb_build_object('category',c.category,'version',c.version,'reason',c.reason,'updated_at',c.updated_at,'updated_by_name',u.name) FROM cash_flow_classifications c JOIN users u ON u.id=c.updated_by WHERE c.journal_id=$1")
        .bind(id).fetch_one(&mut *tx).await?;
    crate::business::audit(
        &mut tx,
        &user,
        "cash-flow.classified",
        "journal",
        id,
        json!({"previous_category":previous,"classification":result}),
    )
    .await?;
    crate::business::remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Filter {
    from: Option<String>,
    to: Option<String>,
}

fn calendar(value: &str) -> AppResult<NaiveDate> {
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| AppError::bad_request("Report dates must use YYYY-MM-DD"))?;
    if !(1..=9999).contains(&date.year()) || date.format("%Y-%m-%d").to_string() != value {
        return Err(AppError::bad_request("Report dates must use YYYY-MM-DD"));
    }
    Ok(date)
}

fn amount(mut value: Decimal) -> String {
    value.rescale(2);
    value.to_string()
}

// Only classifications supported by the chart's existing metadata are inferred.
// A custom asset may be equipment or a deposit, and a custom liability may be a
// bank loan or an operating accrual. Their names/codes cannot establish that.
fn counterpart_category(code: &str, kind: &str, role: Option<&str>) -> Option<&'static str> {
    if matches!(code, "1200" | "1300")
        || matches!(kind, "revenue" | "expense")
        || matches!(role, Some("purchase_payable" | "purchase_receipt_accrual"))
    {
        Some("operating")
    } else if kind == "equity" {
        Some("financing")
    } else {
        None
    }
}

async fn reports(
    State(state): State<AppState>,
    _user: AuthUser,
    Query(filter): Query<Filter>,
) -> AppResult<Json<Value>> {
    // Validate supplied text before handing it to PostgreSQL, including NULs.
    let requested_from = filter.from.as_deref().map(calendar).transpose()?;
    let requested_to = filter.to.as_deref().map(calendar).transpose()?;
    let mut tx = state.db.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    let company = sqlx::query("SELECT name,timezone,(now() AT TIME ZONE timezone)::date AS today,to_jsonb(now()) AS as_of FROM company")
        .fetch_one(&mut *tx).await?;
    let timezone: String = company.get("timezone");
    let to = requested_to.unwrap_or_else(|| company.get("today"));
    let from = requested_from.unwrap_or_else(|| to.with_day(1).expect("first day exists"));
    if from > to {
        return Err(AppError::bad_request(
            "Report from date must be on or before to date",
        ));
    }

    let rows = sqlx::query(r#"
        WITH activity AS (
            SELECT l.account_code,
                sum(l.debit-l.credit) AS cumulative,
                COALESCE(sum(l.debit-l.credit) FILTER (WHERE (j.created_at AT TIME ZONE $1)::date >= $2),0) AS period
            FROM journal_lines l JOIN journal_entries j ON j.id=l.journal_id
            WHERE (j.created_at AT TIME ZONE $1)::date <= $3 GROUP BY l.account_code
        )
        SELECT a.code,a.name,a.kind,COALESCE(x.cumulative,0) AS cumulative,
            COALESCE(x.period,0) AS period FROM accounts a LEFT JOIN activity x ON x.account_code=a.code ORDER BY a.code
    "#).bind(&timezone).bind(from).bind(to).fetch_all(&mut *tx).await?;
    let mut revenue = Vec::new();
    let mut expenses = Vec::new();
    let mut assets = Vec::new();
    let mut liabilities = Vec::new();
    let mut equity = Vec::new();
    let mut retained_accounts = Vec::new();
    let mut revenue_total = Decimal::ZERO;
    let mut expense_total = Decimal::ZERO;
    let mut assets_total = Decimal::ZERO;
    let mut liabilities_total = Decimal::ZERO;
    let mut equity_total = Decimal::ZERO;
    let mut retained = Decimal::ZERO;
    let mut ending_cash = Decimal::ZERO;
    let mut beginning_cash = Decimal::ZERO;
    for row in rows {
        let code: String = row.get("code");
        let kind: String = row.get("kind");
        let cumulative: Decimal = row.get("cumulative");
        let period: Decimal = row.get("period");
        let natural = if matches!(kind.as_str(), "liability" | "equity" | "revenue") {
            -cumulative
        } else {
            cumulative
        };
        let mut item = json!({"code":code,"name":row.get::<String,_>("name"),"kind":kind,"amount":amount(natural)});
        match kind.as_str() {
            "revenue" | "expense" => {
                retained += -cumulative;
                retained_accounts.push(item.clone());
                if kind == "revenue" {
                    revenue_total -= period;
                    item["amount"] = json!(amount(-period));
                    revenue.push(item);
                } else {
                    expense_total += period;
                    item["amount"] = json!(amount(period));
                    expenses.push(item);
                }
            }
            "asset" => {
                assets_total += natural;
                assets.push(item);
            }
            "liability" => {
                liabilities_total += natural;
                liabilities.push(item);
            }
            "equity" => {
                equity_total += natural;
                equity.push(item);
            }
            _ => unreachable!("account kind constrained by schema"),
        }
        if code == "1100" {
            ending_cash = cumulative;
            beginning_cash = cumulative - period;
        }
    }
    let cash_rows = sqlx::query(r#"
        WITH cash AS (
            SELECT j.id,j.source_type,j.source_id,j.description,j.created_at,
                COALESCE(r.source_type,j.source_type) AS original_source_type,
                sum(l.debit) AS cash_in,sum(l.credit) AS cash_out
            FROM journal_entries j JOIN journal_lines l ON l.journal_id=j.id AND l.account_code='1100'
            LEFT JOIN posting_reversals r ON j.source_type='reversal' AND r.id=j.source_id
            WHERE (j.created_at AT TIME ZONE $1)::date BETWEEN $2 AND $3
            GROUP BY j.id,r.source_type
        )
        SELECT c.*,to_jsonb(c.created_at) AS at,
            (SELECT jsonb_build_object('category',m.category,'version',m.version,'reason',m.reason,
                'updated_at',m.updated_at,'updated_by_name',u.name,'history',
                (SELECT jsonb_agg(jsonb_build_object('category',e.category,'previous_category',e.previous_category,
                    'version',e.version,'reason',e.reason,'created_at',e.created_at,'created_by_name',eu.name) ORDER BY e.version DESC)
                 FROM cash_flow_classification_events e JOIN users eu ON eu.id=e.created_by WHERE e.journal_id=c.id))
             FROM cash_flow_classifications m JOIN users u ON u.id=m.updated_by WHERE m.journal_id=c.id) AS classification,
            COALESCE((SELECT jsonb_agg(jsonb_build_object('code',x.code,'name',x.name,'kind',x.kind,
                'system_role',x.system_role,'debit',x.debit::text,'credit',x.credit::text) ORDER BY x.code)
                FROM (SELECT a.code,a.name,a.kind,a.system_role,sum(l.debit) AS debit,sum(l.credit) AS credit
                    FROM journal_lines l JOIN accounts a ON a.code=l.account_code
                    WHERE l.journal_id=c.id AND l.account_code<>'1100' GROUP BY a.code) x),'[]'::jsonb) AS counterparts
        FROM cash c ORDER BY c.created_at,c.id
    "#).bind(&timezone).bind(from).bind(to).fetch_all(&mut *tx).await?;
    let mut totals = [[Decimal::ZERO; 2]; 4];
    let mut entries = Vec::new();
    for row in cash_rows {
        let cash_in: Decimal = row.get("cash_in");
        let cash_out: Decimal = row.get("cash_out");
        let counterparts: Value = row.get("counterparts");
        let source: String = row.get("original_source_type");
        let mut mapped = None;
        let mut certain = true;
        for account in counterparts.as_array().expect("SQL array") {
            let category = counterpart_category(
                account["code"].as_str().unwrap_or_default(),
                account["kind"].as_str().unwrap_or_default(),
                account["system_role"].as_str(),
            );
            if category.is_none() || mapped.is_some_and(|m| Some(m) != category) {
                certain = false;
            }
            mapped = category.or(mapped);
        }
        let classification: Option<Value> = row.get("classification");
        let manual = classification.as_ref().and_then(|m| m["category"].as_str());
        let (key, reason) = if let Some(manual) = manual {
            (manual, "manual_override")
        } else if source == "opening" {
            ("unclassified", "opening_balance")
        } else if cash_in > Decimal::ZERO && cash_out > Decimal::ZERO {
            ("unclassified", "cash_both_directions")
        } else if certain && mapped.is_some() {
            (mapped.unwrap_or("unclassified"), "counterpart_mapping")
        } else {
            ("unclassified", "mixed_or_unmapped_counterparts")
        };
        let index = match key {
            "operating" => 0,
            "investing" => 1,
            "financing" => 2,
            _ => 3,
        };
        totals[index][0] += cash_in;
        totals[index][1] += cash_out;
        entries.push(json!({"journal_id":row.get::<uuid::Uuid,_>("id"),"source_type":row.get::<String,_>("source_type"),
            "source_id":row.get::<uuid::Uuid,_>("source_id"),"original_source_type":source,"description":row.get::<String,_>("description"),
            "created_at":row.get::<Value,_>("at"),"cash_in":amount(cash_in),"cash_out":amount(cash_out),
            "net_change":amount(cash_in-cash_out),"category":key,"classification_reason":reason,"classification":classification,"counterparts":counterparts}));
    }
    let categories: Vec<Value> = [
        ("operating", "Operasi"), ("investing", "Investasi"),
        ("financing", "Pendanaan"), ("unclassified", "Belum terklasifikasi / saldo awal"),
    ].into_iter().enumerate().map(|(i,(key,name))| json!({"key":key,"name":name,
        "cash_in":amount(totals[i][0]),"cash_out":amount(totals[i][1]),"net_change":amount(totals[i][0]-totals[i][1])})).collect();
    let cash_in: Decimal = totals.iter().map(|t| t[0]).sum();
    let cash_out: Decimal = totals.iter().map(|t| t[1]).sum();
    let cash_difference = ending_cash - beginning_cash - cash_in + cash_out;
    let liabilities_and_equity = liabilities_total + equity_total + retained;
    let difference = assets_total - liabilities_and_equity;
    let result = json!({"from":from,"to":to,"timezone":timezone,"as_of":company.get::<Value,_>("as_of"),"company_name":company.get::<String,_>("name"),
        "income_statement":{"revenue_accounts":revenue,"expense_accounts":expenses,"revenue_total":amount(revenue_total),"expense_total":amount(expense_total),"net_profit":amount(revenue_total-expense_total)},
        "balance_sheet":{"asset_accounts":assets,"liability_accounts":liabilities,"equity_accounts":equity,
            "retained_earnings_accounts":retained_accounts,"assets_total":amount(assets_total),"liabilities_total":amount(liabilities_total),
            "equity_total":amount(equity_total),"retained_earnings":amount(retained),"liabilities_and_equity_total":amount(liabilities_and_equity),
            "difference":amount(difference),"balanced":difference==Decimal::ZERO},
        "cash_flow":{"account_code":"1100","beginning_balance":amount(beginning_cash),"ending_balance":amount(ending_cash),
            "cash_in":amount(cash_in),"cash_out":amount(cash_out),"net_change":amount(cash_in-cash_out),"difference":amount(cash_difference),
            "reconciled":cash_difference==Decimal::ZERO,"categories":categories,"entries":entries}});
    tx.commit().await?;
    Ok(Json(result))
}
