#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::{Decimal, Rat};

fn dec(text: &str) -> Decimal {
    Decimal::parse(text).unwrap()
}

#[test]
fn decimal_arithmetic() {
    assert_eq!(dec("0.5").checked_add(dec("0.25")), Some(dec("0.75")));
    assert_eq!(dec("1.0"), Decimal::ONE);
    assert_eq!(dec("1.50"), dec("1.5"));
    assert_eq!(dec("1.5").checked_mul(dec("1.5")), Some(dec("2.25")));
    assert_eq!(
        dec("0.1").checked_sub(dec("0.35")),
        dec("0.25").checked_neg()
    );
    assert_eq!(
        dec("0.1").checked_sub(dec("0.35")).unwrap().to_string(),
        "-0.25"
    );
    assert_eq!(dec("1_000.0").to_string(), "1000.0");
    assert_eq!(dec("0.05").to_string(), "0.05");
    assert_eq!(dec("12.250").to_string(), "12.25");
    assert_eq!(
        dec("0.5").checked_cmp(dec("0.25")),
        Some(std::cmp::Ordering::Greater)
    );
    assert_eq!(
        dec("0.1").checked_neg().unwrap().checked_cmp(dec("0.0")),
        Some(std::cmp::Ordering::Less)
    );
    assert_eq!(dec("1").checked_div(dec("4")), Ok(dec("0.25")));
    assert_eq!(dec("1").checked_div(dec("3")), Ok(dec("0.3333333")));
    assert_eq!(
        dec("2").checked_neg().unwrap().checked_div(dec("3")),
        dec("0.6666666").checked_neg().ok_or(false)
    );
    assert_eq!(dec("1").checked_div(dec("0.0")), Err(true));
    assert!(dec("0.0").is_fraction() && dec("1.0").is_fraction() && dec("0.7").is_fraction());
    assert!(!dec("1.01").is_fraction());
    assert!(!dec("0.01").checked_neg().unwrap().is_fraction());
    assert_eq!(Decimal::new(i128::MAX, 0).checked_add(Decimal::ONE), None);
    assert_eq!(Decimal::parse("1.2.3"), None);
}

#[test]
fn rat_arithmetic() {
    let half = Rat::new(2, 4).unwrap();
    assert_eq!((half.numer(), half.denom()), (1, 2));
    assert_eq!(Rat::new(1, -2), Rat::new(-1, 2));
    assert_eq!(Rat::new(1, 0), None);
    let third = Rat::new(1, 3).unwrap();
    assert_eq!(half.checked_add(third), Rat::new(5, 6));
    assert_eq!(half.checked_sub(third), Rat::new(1, 6));
    assert_eq!(half.checked_mul(third), Rat::new(1, 6));
    assert_eq!(half.checked_div(third), Rat::new(3, 2));
    assert_eq!(third.checked_cmp(half), Some(std::cmp::Ordering::Less));
    assert_eq!(Rat::new(1, 8).unwrap().to_decimal(), Some(dec("0.125")));
    assert_eq!(third.to_decimal(), Some(dec("0.3333333")));
    assert_eq!(
        Rat::new(-2, 3).unwrap().to_decimal(),
        dec("0.6666666").checked_neg()
    );
    assert_eq!(Rat::from_decimal(dec("0.75")), Rat::new(3, 4));
    assert_eq!(Rat::new(-3, 4).unwrap().to_string(), "-3/4");
    assert_eq!(Rat::from_int(5).to_string(), "5");
}
