use std::{collections::BTreeSet, str::FromStr};

use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    AppState,
    auth::AuthUser,
    business::{audit, cached, parse, payload, remember, text},
    corrections::check_open,
    domain,
    error::{AppError, AppResult},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/accounting/opening", get(read).put(save))
        .route("/accounting/opening/post", post(post_opening))
        .route("/accounting/opening/reverse", post(reverse_opening))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ItemInput {
    product_id: Uuid,
    quantity: String,
    unit_cost: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LineInput {
    account_code: String,
    debit: String,
    credit: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DraftInput {
    version: i32,
    description: String,
    items: Vec<ItemInput>,
    lines: Vec<LineInput>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PostInput {
    version: i32,
    idempotency_key: Uuid,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReverseInput {
    version: i32,
    reason: String,
    idempotency_key: Uuid,
}

#[derive(Clone, Deserialize, Serialize)]
struct Item {
    product_id: Uuid,
    sku: String,
    name: String,
    unit: String,
    quantity: String,
    unit_cost: String,
    value: String,
}

#[derive(Clone, Deserialize, Serialize)]
struct Line {
    account_code: String,
    account_name: String,
    debit: String,
    credit: String,
}

struct Draft {
    items: Vec<Item>,
    lines: Vec<Line>,
    inventory: Decimal,
    debit: Decimal,
    credit: Decimal,
}

fn number(value: &str) -> AppResult<Decimal> {
    Decimal::from_str(value).map_err(|_| AppError::bad_request("Invalid stored decimal value"))
}

fn nonnegative(value: &str) -> AppResult<Decimal> {
    let amount = number(value)?;
    if amount < Decimal::ZERO
        || amount > Decimal::from(1_000_000_000_000u64)
        || amount.normalize().scale() > 2
    {
        return Err(AppError::bad_request(
            "Opening debit and credit must be nonnegative, at most 1000000000000, with at most 2 decimal places",
        ));
    }
    Ok(amount)
}

fn too_large(debit: Decimal, credit: Decimal) -> bool {
    debit > Decimal::from(1_000_000_000_000u64) || credit > Decimal::from(1_000_000_000_000u64)
}

// All ordinary postings acquire the same accounting lock first. Draft saves use
// it too, so the initial-state check cannot race a receipt or a first journal.
async fn lock_accounting(tx: &mut Transaction<'_, Postgres>) -> AppResult<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(8414077)")
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn lock_document(
    tx: &mut Transaction<'_, Postgres>,
) -> AppResult<Option<sqlx::postgres::PgRow>> {
    Ok(sqlx::query("SELECT id,status,version,description,items,lines,active_posting_id FROM opening_documents WHERE company_id=(SELECT id FROM company) FOR UPDATE")
        .fetch_optional(&mut **tx).await?)
}

fn require_version(row: &sqlx::postgres::PgRow, version: i32) -> AppResult<()> {
    if row.get::<i32, _>("version") != version {
        return Err(AppError::conflict(
            "Opening balances changed; reload before continuing",
        ));
    }
    Ok(())
}

// Only this document's historical opening and exact reversal postings are
// exempt. A business posting remains a blocker even after it was reversed.
async fn business_exists(tx: &mut Transaction<'_, Postgres>) -> AppResult<bool> {
    Ok(sqlx::query_scalar(r#"SELECT
        EXISTS(SELECT 1 FROM journal_entries j WHERE NOT (
            (j.source_type='opening' AND EXISTS(SELECT 1 FROM opening_postings p WHERE p.id=j.source_id AND p.journal_id=j.id)) OR
            (j.source_type='reversal' AND EXISTS(SELECT 1 FROM posting_reversals r JOIN opening_postings p ON p.id=r.source_id
                WHERE r.source_type='opening' AND r.id=j.source_id)))) OR
        EXISTS(SELECT 1 FROM inventory_movements m WHERE NOT (
            m.kind IN ('opening','opening_reversal') AND EXISTS(SELECT 1 FROM opening_postings p WHERE p.id=m.opening_posting_id)))"#)
        .fetch_one(&mut **tx).await?)
}

async fn clean_draft(tx: &mut Transaction<'_, Postgres>) -> AppResult<()> {
    if business_exists(tx).await? {
        return Err(AppError::conflict(
            "Opening balances require a company without existing business journals or inventory movements",
        ));
    }
    let nonzero: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM products WHERE quantity<>0 OR inventory_value<>0 OR average_cost<>0)")
        .fetch_one(&mut **tx).await?;
    if nonzero {
        return Err(AppError::conflict(
            "Opening balances require zero inventory before initialization",
        ));
    }
    Ok(())
}

async fn validate(
    tx: &mut Transaction<'_, Postgres>,
    items: &[ItemInput],
    lines: &[LineInput],
) -> AppResult<Draft> {
    if items.len() > 100 || lines.len() > 100 {
        return Err(AppError::bad_request(
            "Opening balances allow at most 100 stock items and 100 account lines",
        ));
    }
    let mut product_ids = BTreeSet::new();
    for item in items {
        if !product_ids.insert(item.product_id) {
            return Err(AppError::bad_request("Duplicate opening product"));
        }
    }
    // Match product-edit and order locking: sorted identifiers avoid cycles.
    let product_ids: Vec<_> = product_ids.into_iter().collect();
    sqlx::query("SELECT id FROM products WHERE id=ANY($1) ORDER BY id FOR UPDATE")
        .bind(&product_ids)
        .fetch_all(&mut **tx)
        .await?;
    let mut result = Draft {
        items: Vec::new(),
        lines: Vec::new(),
        inventory: Decimal::ZERO,
        debit: Decimal::ZERO,
        credit: Decimal::ZERO,
    };
    for item in items {
        let product = sqlx::query("SELECT sku,name,unit,kind FROM products WHERE id=$1")
            .bind(item.product_id)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(|| AppError::bad_request("Unknown opening product"))?;
        if product.get::<String, _>("kind") != "stock" {
            return Err(AppError::bad_request(
                "Services cannot have opening inventory",
            ));
        }
        let quantity = parse(&item.quantity, 3, "Opening quantity")?;
        let cost = parse(&item.unit_cost, 4, "Opening unit cost")?;
        let value = domain::money(quantity * cost);
        if value <= Decimal::ZERO || value > Decimal::from(1_000_000_000_000u64) {
            return Err(AppError::bad_request(
                "Rounded opening item value must be positive and at most 1000000000000",
            ));
        }
        result.inventory += value;
        result.items.push(Item {
            product_id: item.product_id,
            sku: product.get("sku"),
            name: product.get("name"),
            unit: product.get("unit"),
            quantity: quantity.to_string(),
            unit_cost: cost.to_string(),
            value: value.to_string(),
        });
    }
    let mut accounts = BTreeSet::new();
    for line in lines {
        if !accounts.insert(line.account_code.clone()) {
            return Err(AppError::bad_request("Duplicate opening account"));
        }
        if line.account_code == "1200" || line.account_code == "1300" {
            return Err(AppError::bad_request(
                "Opening receivables require customer documents; inventory is generated from opening stock items",
            ));
        }
        let account = sqlx::query("SELECT name,kind,system_role FROM accounts WHERE code=$1")
            .bind(&line.account_code)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(|| AppError::bad_request("Unknown opening account"))?;
        if account.get::<Option<String>, _>("system_role").is_some() {
            return Err(AppError::bad_request(
                "Opening control balances require their source document workflow",
            ));
        }
        if !matches!(
            account.get::<String, _>("kind").as_str(),
            "asset" | "liability" | "equity"
        ) {
            return Err(AppError::bad_request(
                "Opening lines require asset, liability or equity accounts",
            ));
        }
        let debit = nonnegative(&line.debit)?;
        let credit = nonnegative(&line.credit)?;
        if (debit > Decimal::ZERO) == (credit > Decimal::ZERO) {
            return Err(AppError::bad_request(
                "Each opening line must contain exactly one positive debit or credit",
            ));
        }
        result.debit += debit;
        result.credit += credit;
        result.lines.push(Line {
            account_code: line.account_code.clone(),
            account_name: account.get("name"),
            debit: debit.to_string(),
            credit: credit.to_string(),
        });
    }
    if result.inventory > Decimal::ZERO {
        let name: String = sqlx::query_scalar("SELECT name FROM accounts WHERE code='1300'")
            .fetch_one(&mut **tx)
            .await?;
        result.lines.push(Line {
            account_code: "1300".into(),
            account_name: name,
            debit: result.inventory.to_string(),
            credit: "0".into(),
        });
        result.debit += result.inventory;
    }
    if too_large(result.debit, result.credit) {
        return Err(AppError::bad_request(
            "Opening journal totals must be at most 1000000000000",
        ));
    }
    Ok(result)
}

fn draft_inputs(row: &sqlx::postgres::PgRow) -> AppResult<(Vec<ItemInput>, Vec<LineInput>)> {
    let items: Vec<Item> =
        serde_json::from_value(row.get("items")).map_err(|e| AppError::internal(e.into()))?;
    let lines: Vec<Line> =
        serde_json::from_value(row.get("lines")).map_err(|e| AppError::internal(e.into()))?;
    Ok((
        items
            .into_iter()
            .map(|i| ItemInput {
                product_id: i.product_id,
                quantity: i.quantity,
                unit_cost: i.unit_cost,
            })
            .collect(),
        lines
            .into_iter()
            .filter(|l| l.account_code != "1300")
            .map(|l| LineInput {
                account_code: l.account_code,
                debit: l.debit,
                credit: l.credit,
            })
            .collect(),
    ))
}

async fn reversible(tx: &mut Transaction<'_, Postgres>, posting_id: Uuid) -> AppResult<bool> {
    let period_open: bool = sqlx::query_scalar("SELECT closed_through IS NULL OR (now() AT TIME ZONE (SELECT timezone FROM company))::date>closed_through FROM accounting_period WHERE singleton")
        .fetch_one(&mut **tx).await?;
    if !period_open || business_exists(tx).await? {
        return Ok(false);
    }
    // Guard against external inventory edits too. Products not in the opening
    // must remain empty; opening products must retain exact carrying values.
    let mismatch:bool=sqlx::query_scalar(r#"SELECT EXISTS(
        SELECT 1 FROM products p LEFT JOIN
            (SELECT (i->>'product_id')::uuid product_id,(i->>'quantity')::numeric quantity,(i->>'value')::numeric value
             FROM opening_postings o CROSS JOIN LATERAL jsonb_array_elements(o.items) i WHERE o.id=$1) s ON s.product_id=p.id
        WHERE p.quantity<>COALESCE(s.quantity,0) OR p.inventory_value<>COALESCE(s.value,0)
            OR p.average_cost<>CASE WHEN s.quantity>0 THEN round(s.value/s.quantity,4) ELSE 0 END)"#)
        .bind(posting_id).fetch_one(&mut **tx).await?;
    Ok(!mismatch)
}

async fn envelope(tx: &mut Transaction<'_, Postgres>) -> AppResult<Value> {
    let row=sqlx::query("SELECT id,status,version,description,items,lines,active_posting_id,created_at FROM opening_documents WHERE company_id=(SELECT id FROM company)")
        .fetch_optional(&mut **tx).await?;
    let has_business = business_exists(tx).await?;
    let nonzero:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM products WHERE quantity<>0 OR inventory_value<>0 OR average_cost<>0)")
        .fetch_one(&mut **tx).await?;
    let posted = row
        .as_ref()
        .is_some_and(|r| r.get::<String, _>("status") == "posted");
    let blocked_reason = if has_business {
        Some(
            "Opening balances require a company without existing business journals or inventory movements",
        )
    } else if posted {
        Some("Opening balances are already posted")
    } else if nonzero {
        Some("Opening balances require zero inventory before initialization")
    } else {
        None
    };
    let mut can_reverse = false;
    let document = if let Some(row) = row {
        let id: Uuid = row.get("id");
        let active: Option<Uuid> = row.get("active_posting_id");
        if let Some(active) = active {
            can_reverse = reversible(tx, active).await?;
        }
        let history:Value=sqlx::query_scalar(r#"SELECT COALESCE(jsonb_agg(jsonb_build_object(
            'id',p.id,'journal_id',p.journal_id,'posted_at',p.posted_at,'reversed_at',r.created_at,'reason',r.reason,
            'description',p.description,'items',p.items,'lines',p.lines,'inventory_total',p.inventory_total::text,
            'total_debit',p.total_debit::text,'total_credit',p.total_credit::text) ORDER BY p.posted_at,p.document_version),'[]'::jsonb)
            FROM opening_postings p LEFT JOIN posting_reversals r ON r.source_type='opening' AND r.source_id=p.id WHERE p.document_id=$1"#)
            .bind(id).fetch_one(&mut **tx).await?;
        let items: Vec<Item> =
            serde_json::from_value(row.get("items")).map_err(|e| AppError::internal(e.into()))?;
        let lines: Vec<Line> =
            serde_json::from_value(row.get("lines")).map_err(|e| AppError::internal(e.into()))?;
        let inventory = items
            .iter()
            .try_fold(Decimal::ZERO, |sum, i| number(&i.value).map(|v| sum + v))?;
        let debit = lines
            .iter()
            .try_fold(Decimal::ZERO, |sum, l| number(&l.debit).map(|v| sum + v))?;
        let credit = lines
            .iter()
            .try_fold(Decimal::ZERO, |sum, l| number(&l.credit).map(|v| sum + v))?;
        let posted_at = active.and_then(|a| {
            history
                .as_array()
                .and_then(|h| h.iter().find(|p| p["id"] == a.to_string()))
                .map(|p| p["posted_at"].clone())
        });
        json!({"id":id,"status":row.get::<String,_>("status"),"version":row.get::<i32,_>("version"),
            "description":row.get::<String,_>("description"),"created_at":row.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),
            "posted_at":posted_at,"items":items,"lines":lines,"inventory_total":inventory.to_string(),
            "total_debit":debit.to_string(),"total_credit":credit.to_string(),"balanced":debit>Decimal::ZERO && debit==credit,"history":history})
    } else {
        Value::Null
    };
    Ok(
        json!({"eligible":blocked_reason.is_none(),"blocked_reason":blocked_reason,"can_reverse":can_reverse,"document":document}),
    )
}

async fn read(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<Value>> {
    let mut tx = state.db.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    let result = envelope(&mut tx).await?;
    tx.commit().await?;
    Ok(Json(result))
}

async fn save(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<DraftInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let description = text(&body.description, 500, "Description")?;
    let mut tx = state.db.begin().await?;
    lock_accounting(&mut tx).await?;
    let row = lock_document(&mut tx).await?;
    if let Some(row) = &row {
        require_version(row, body.version)?;
        if row.get::<String, _>("status") != "draft" {
            return Err(AppError::conflict(
                "Reverse opening balances before editing",
            ));
        }
    } else if body.version != 0 {
        return Err(AppError::conflict(
            "Opening balances changed; reload before continuing",
        ));
    }
    clean_draft(&mut tx).await?;
    let draft = validate(&mut tx, &body.items, &body.lines).await?;
    let items = serde_json::to_value(&draft.items).map_err(|e| AppError::internal(e.into()))?;
    let lines = serde_json::to_value(&draft.lines).map_err(|e| AppError::internal(e.into()))?;
    let id = if let Some(row) = row {
        let id: Uuid = row.get("id");
        sqlx::query("UPDATE opening_documents SET description=$2,items=$3,lines=$4,version=version+1 WHERE id=$1")
            .bind(id).bind(description).bind(items).bind(lines).execute(&mut *tx).await?;
        id
    } else {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO opening_documents(id,company_id,version,description,items,lines) VALUES($1,(SELECT id FROM company),1,$2,$3,$4)")
            .bind(id).bind(description).bind(items).bind(lines).execute(&mut *tx).await?;
        id
    };
    audit(&mut tx,&user,"opening.draft_saved","opening_document",id,json!({"item_count":body.items.len(),"line_count":body.lines.len(),"version":body.version+1})).await?;
    let result = envelope(&mut tx).await?;
    tx.commit().await?;
    Ok(Json(result))
}

async fn post_opening(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<PostInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let request = payload("opening.post", &user, &body);
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let row = lock_document(&mut tx)
        .await?
        .ok_or_else(|| AppError::conflict("Save an opening draft before posting"))?;
    require_version(&row, body.version)?;
    if row.get::<String, _>("status") != "draft" {
        return Err(AppError::conflict("Opening balances are already posted"));
    }
    clean_draft(&mut tx).await?;
    let (items, lines) = draft_inputs(&row)?;
    let draft = validate(&mut tx, &items, &lines).await?;
    if draft.debit <= Decimal::ZERO || draft.debit != draft.credit || draft.lines.len() < 2 {
        return Err(AppError::bad_request(
            "Opening balances require at least two balanced lines and a positive total",
        ));
    }
    let id: Uuid = row.get("id");
    let posting_id = Uuid::new_v4();
    let journal_id = Uuid::new_v4();
    let description: String = row.get("description");
    let items = serde_json::to_value(&draft.items).map_err(|e| AppError::internal(e.into()))?;
    let lines = serde_json::to_value(&draft.lines).map_err(|e| AppError::internal(e.into()))?;
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by) VALUES($1,$2,'opening',$3,$4)")
        .bind(journal_id).bind(&description).bind(posting_id).bind(user.id).execute(&mut *tx).await?;
    for line in &draft.lines {
        sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) VALUES($1,$2,$3,$4,$5)")
            .bind(Uuid::new_v4()).bind(journal_id).bind(&line.account_code).bind(number(&line.debit)?).bind(number(&line.credit)?).execute(&mut *tx).await?;
    }
    sqlx::query("INSERT INTO opening_postings(id,document_id,document_version,journal_id,description,items,lines,inventory_total,total_debit,total_credit,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)")
        .bind(posting_id).bind(id).bind(body.version).bind(journal_id).bind(&description).bind(&items).bind(&lines).bind(draft.inventory).bind(draft.debit).bind(draft.credit).bind(user.id).execute(&mut *tx).await?;
    for item in &draft.items {
        let quantity = number(&item.quantity)?;
        let value = number(&item.value)?;
        sqlx::query(
            "UPDATE products SET quantity=$2,inventory_value=$3,average_cost=$4 WHERE id=$1",
        )
        .bind(item.product_id)
        .bind(quantity)
        .bind(value)
        .bind(domain::average(value, quantity))
        .execute(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO inventory_movements(id,product_id,kind,quantity,unit_cost,value,reference,created_by,opening_posting_id) VALUES($1,$2,'opening',$3,$4,$5,$6,$7,$8)")
            .bind(Uuid::new_v4()).bind(item.product_id).bind(quantity).bind(number(&item.unit_cost)?).bind(value).bind(&description).bind(user.id).bind(posting_id).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE opening_documents SET status='posted',active_posting_id=$2,items=$3,lines=$4,version=version+1 WHERE id=$1")
        .bind(id).bind(posting_id).bind(items).bind(lines).execute(&mut *tx).await?;
    audit(
        &mut tx,
        &user,
        "opening.posted",
        "opening_document",
        id,
        json!({"posting_id":posting_id,"journal_id":journal_id,"total":draft.debit.to_string()}),
    )
    .await?;
    let result = envelope(&mut tx).await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}

async fn reverse_opening(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<ReverseInput>,
) -> AppResult<Json<Value>> {
    user.require(&["finance"])?;
    let request = payload("opening.reverse", &user, &body);
    let mut tx = state.db.begin().await?;
    if let Some(result) = cached(&mut tx, body.idempotency_key, &request).await? {
        tx.commit().await?;
        return Ok(Json(result));
    }
    check_open(&mut tx).await?;
    let reason = text(&body.reason, 1000, "Reason")?;
    if reason.chars().count() < 3 {
        return Err(AppError::bad_request(
            "Reason must contain at least 3 characters",
        ));
    }
    let row = lock_document(&mut tx)
        .await?
        .ok_or_else(|| AppError::conflict("Opening balances are not posted"))?;
    require_version(&row, body.version)?;
    let posting_id: Uuid = row
        .get::<Option<Uuid>, _>("active_posting_id")
        .ok_or_else(|| AppError::conflict("Opening balances are not posted"))?;
    // Lock all current product rows in the same sorted order as ordinary stock
    // operations, before verifying exact inventory and applying the reversal.
    sqlx::query("SELECT id FROM products ORDER BY id FOR UPDATE")
        .fetch_all(&mut *tx)
        .await?;
    if !reversible(&mut tx, posting_id).await? {
        return Err(AppError::conflict(
            "Opening balances cannot be reversed after other business postings or inventory changes",
        ));
    }
    let posting = sqlx::query("SELECT journal_id,description FROM opening_postings WHERE id=$1")
        .bind(posting_id)
        .fetch_one(&mut *tx)
        .await?;
    let reversal_id = Uuid::new_v4();
    sqlx::query("INSERT INTO posting_reversals(id,source_type,source_id,reason,created_by) VALUES($1,'opening',$2,$3,$4)")
        .bind(reversal_id).bind(posting_id).bind(&reason).bind(user.id).execute(&mut *tx).await?;
    let journal_id = Uuid::new_v4();
    let description = format!("Reverse opening balances: {reason}");
    sqlx::query("INSERT INTO journal_entries(id,description,source_type,source_id,created_by) VALUES($1,$2,'reversal',$3,$4)")
        .bind(journal_id).bind(&description).bind(reversal_id).bind(user.id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO journal_lines(id,journal_id,account_code,debit,credit) SELECT gen_random_uuid(),$1,account_code,credit,debit FROM journal_lines WHERE journal_id=$2")
        .bind(journal_id).bind(posting.get::<Uuid,_>("journal_id")).execute(&mut *tx).await?;
    let movements=sqlx::query("SELECT id,product_id,quantity,unit_cost,value FROM inventory_movements WHERE opening_posting_id=$1 AND kind='opening' ORDER BY product_id")
        .bind(posting_id).fetch_all(&mut *tx).await?;
    for movement in movements {
        let product_id: Uuid = movement.get("product_id");
        sqlx::query("UPDATE products SET quantity=0,inventory_value=0,average_cost=0 WHERE id=$1")
            .bind(product_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO inventory_movements(id,product_id,kind,quantity,unit_cost,value,reference,created_by,reverses_id,opening_posting_id) VALUES($1,$2,'opening_reversal',$3,$4,$5,$6,$7,$8,$9)")
            .bind(Uuid::new_v4()).bind(product_id).bind(-movement.get::<Decimal,_>("quantity")).bind(movement.get::<Decimal,_>("unit_cost")).bind(movement.get::<Decimal,_>("value")).bind(&reason).bind(user.id).bind(movement.get::<Uuid,_>("id")).bind(posting_id).execute(&mut *tx).await?;
    }
    let id: Uuid = row.get("id");
    sqlx::query("UPDATE opening_documents SET status='draft',active_posting_id=NULL,version=version+1 WHERE id=$1")
        .bind(id).execute(&mut *tx).await?;
    audit(&mut tx,&user,"opening.reversed","opening_document",id,json!({"posting_id":posting_id,"reversal_id":reversal_id,"journal_id":journal_id,"reason":reason})).await?;
    let result = envelope(&mut tx).await?;
    remember(&mut tx, body.idempotency_key, &request, &result).await?;
    tx.commit().await?;
    Ok(Json(result))
}
