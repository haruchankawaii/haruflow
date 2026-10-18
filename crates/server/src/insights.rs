use axum::{
    Json, Router,
    extract::{Query, State},
    routing::get,
};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    AppState,
    auth::AuthUser,
    error::{AppError, AppResult},
};

pub fn router() -> Router<AppState> {
    Router::new().route("/dashboard/insights", get(insights))
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Filter {
    months: Option<u8>,
}

async fn insights(
    State(state): State<AppState>,
    _user: AuthUser,
    Query(filter): Query<Filter>,
) -> AppResult<Json<Value>> {
    let months = filter.months.unwrap_or(6);
    if ![3, 6, 12].contains(&months) {
        return Err(AppError::bad_request(
            "Chart range must be 3, 6, or 12 months",
        ));
    }
    // One statement gives all series, totals, and the current invoice position
    // the same database snapshot. Corrections affect their posting month; an
    // opening balance contributes to cash movements but never to revenue.
    let result = sqlx::query_scalar::<_, Value>(
        r#"
        WITH bounds AS (
            SELECT timezone AS tz, now() AS as_of,
                date_trunc('month',now() AT TIME ZONE timezone) AS current_month
            FROM company
        ), months AS (
            SELECT generate_series(
                current_month - ($1::int-1)*interval '1 month',
                current_month, interval '1 month') AS starts
            FROM bounds
        ), activity AS (
            SELECT date_trunc('month',j.created_at AT TIME ZONE b.tz) AS starts,
                sum(CASE WHEN l.account_code='4000' THEN l.credit-l.debit ELSE 0 END) AS revenue,
                sum(CASE WHEN l.account_code='5000' THEN l.debit-l.credit ELSE 0 END) AS cogs,
                sum(CASE WHEN l.account_code='1100' THEN l.debit ELSE 0 END) AS cash_in,
                sum(CASE WHEN l.account_code='1100' THEN l.credit ELSE 0 END) AS cash_out
            FROM journal_entries j JOIN journal_lines l ON l.journal_id=j.id CROSS JOIN bounds b
            WHERE j.created_at >= (b.current_month-($1::int-1)*interval '1 month') AT TIME ZONE b.tz
                AND j.created_at<=b.as_of AND l.account_code IN ('1100','4000','5000')
            GROUP BY 1
        ), series AS (
            SELECT m.starts,COALESCE(a.revenue,0.00) AS revenue,COALESCE(a.cogs,0.00) AS cogs,
                COALESCE(a.cash_in,0.00) AS cash_in,COALESCE(a.cash_out,0.00) AS cash_out
            FROM months m LEFT JOIN activity a ON a.starts=m.starts
        ), totals AS (
            SELECT sum(revenue) AS revenue,sum(cogs) AS cogs,
                sum(cash_in) AS cash_in,sum(cash_out) AS cash_out FROM series
        ), receivables AS (
            SELECT COALESCE(sum(total),0.00) AS invoiced,COALESCE(sum(paid),0.00) AS paid,
                COALESCE(sum(total-paid),0.00) AS outstanding,
                count(*) FILTER (WHERE status='unpaid') AS unpaid_count,
                count(*) FILTER (WHERE status='partial') AS partial_count,
                count(*) FILTER (WHERE status='paid') AS paid_count
            FROM invoices WHERE status<>'void'
        )
        SELECT jsonb_build_object(
            'months',$1::int,'timezone',b.tz,'from',to_char(b.current_month-($1::int-1)*interval '1 month','YYYY-MM-DD'),
            'through',to_char(b.as_of AT TIME ZONE b.tz,'YYYY-MM-DD'),'as_of',b.as_of,
            'series',(SELECT jsonb_agg(jsonb_build_object(
                'month',to_char(starts,'YYYY-MM'),'revenue',revenue::text,'cogs',cogs::text,
                'gross_profit',(revenue-cogs)::text,'cash_in',cash_in::text,'cash_out',cash_out::text,
                'cash_net',(cash_in-cash_out)::text) ORDER BY starts) FROM series),
            'totals',jsonb_build_object('revenue',t.revenue::text,'cogs',t.cogs::text,
                'gross_profit',(t.revenue-t.cogs)::text,'cash_in',t.cash_in::text,
                'cash_out',t.cash_out::text,'cash_net',(t.cash_in-t.cash_out)::text),
            'receivables',jsonb_build_object('invoiced',r.invoiced::text,'paid',r.paid::text,
                'outstanding',r.outstanding::text,'unpaid_count',r.unpaid_count,
                'partial_count',r.partial_count,'paid_count',r.paid_count))
        FROM bounds b CROSS JOIN totals t CROSS JOIN receivables r
        "#,
    )
    .bind(i32::from(months))
    .fetch_one(&state.db)
    .await?;
    Ok(Json(result))
}
