use rust_decimal::{Decimal, RoundingStrategy};
use std::str::FromStr;

pub fn decimal(input: &str, scale: u32, label: &str) -> Result<Decimal, String> {
    let value =
        Decimal::from_str(input).map_err(|_| format!("{label} must be a decimal string"))?;
    if value <= Decimal::ZERO
        || value > Decimal::from(1_000_000_000u64)
        || value.normalize().scale() > scale
    {
        return Err(format!(
            "{label} must be positive, at most 1000000000, with at most {scale} decimal places"
        ));
    }
    Ok(value)
}

pub fn money(value: Decimal) -> Decimal {
    value.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

pub fn average(value: Decimal, quantity: Decimal) -> Decimal {
    if quantity == Decimal::ZERO {
        Decimal::ZERO
    } else {
        (value / quantity).round_dp_with_strategy(4, RoundingStrategy::MidpointAwayFromZero)
    }
}

pub fn shipment_cost(
    quantity: Decimal,
    on_hand: Decimal,
    average_cost: Decimal,
    carrying_value: Decimal,
) -> Decimal {
    if quantity == on_hand {
        carrying_value
    } else {
        money(quantity * average_cost).min(carrying_value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn d(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    #[test]
    fn validates_scale_without_using_floats() {
        assert_eq!(decimal("1.2300", 2, "price").unwrap(), d("1.23"));
        for invalid in ["0", "-1", "NaN", "1000000001", "0.001"] {
            assert!(decimal(invalid, 2, "price").is_err());
        }
    }

    #[test]
    fn monetary_rounding_is_explicit() {
        assert_eq!(money(d("1.005")), d("1.01"));
        assert_eq!(money(d("0.004")), Decimal::ZERO);
    }

    #[test]
    fn moving_average_and_final_shipment_preserve_carrying_value() {
        let quantity = d("3");
        let value = d("10");
        let cost = average(value, quantity);
        assert_eq!(cost, d("3.3333"));
        let first = shipment_cost(d("1"), quantity, cost, value);
        let second = shipment_cost(d("1"), d("2"), cost, value - first);
        let last = shipment_cost(d("1"), d("1"), cost, value - first - second);
        assert_eq!(first + second + last, value);
        assert_eq!(last, d("3.34"));
    }

    #[test]
    fn partial_shipment_cannot_overdraw_rounded_inventory_value() {
        assert_eq!(
            shipment_cost(d("0.999"), d("1"), d("0.01"), d("0.01")),
            d("0.01")
        );
        assert_eq!(average(Decimal::ZERO, Decimal::ZERO), Decimal::ZERO);
    }

    #[test]
    fn reversal_after_another_receipt_restores_original_carrying_cost() {
        let original_value = d("10.00");
        let removed = shipment_cost(
            d("1"),
            d("3"),
            average(original_value, d("3")),
            original_value,
        );
        let remaining_value = original_value - removed;
        // Receive one more unit for 5.00, then reverse the earlier one-unit shipment.
        let restored_value = remaining_value + d("5.00") + removed;
        assert_eq!(restored_value, d("15.00"));
        assert_eq!(average(restored_value, d("4")), d("3.7500"));
    }
}
