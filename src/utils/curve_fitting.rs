use roots::{find_roots_quadratic, Roots};

/// Linear interpolation between two points. Returns the average y value when `x1 == x2`.
#[must_use]
pub fn linear_interpolation(x1: f64, x2: f64, y1: f64, y2: f64, xp: f64) -> f64 {
    // At the tails of a cumulative rate function, we may hit a point where the cumulative rates are
    // the same, but we still need to be able to do interpolation. We just return the average of the
    // y values in this case (x1 == x2 == xp).
    #[allow(clippy::float_cmp)]
    if x1 == x2 {
        return (y1 + y2) / 2.0;
    }
    y1 + (y2 - y1) / (x2 - x1) * (xp - x1)
}

/// Returns the larger root of a quadratic equation of the form `a2 * x^2 + a1 * x + a0 = 0`.
/// Returns `None` if the discriminant is negative.
#[allow(clippy::missing_panics_doc)]
#[must_use]
pub fn upper_quadratic_root(a2: f64, a1: f64, a0: f64) -> Option<f64> {
    let solutions = find_roots_quadratic(a2, a1, a0);
    match solutions {
        Roots::No([]) => None,
        Roots::One([x]) => Some(x),
        Roots::Two([x1, x2]) => {
            // We want the positive root, so we take the max of the two.
            Some(f64::max(x1, x2))
        }
        // We should never experience this panic because a quadratic function cannot have more
        // than two roots. But, we need a panic because `Roots` has more variants, and `match`
        // must be exhaustive.
        _ => panic!("A quadratic cannot have more than two roots."),
    }
}

#[cfg(test)]
mod test {
    use statrs::assert_almost_eq;

    use super::{linear_interpolation, upper_quadratic_root};

    #[test]
    fn test_linear_interpolation_simple() {
        let result = linear_interpolation(1.0, 2.0, 3.0, 6.0, 1.25);
        assert_almost_eq!(result, 3.75, 0.0);
    }

    #[test]
    fn test_linear_extrapolation_simple() {
        let result = linear_interpolation(1.0, 2.0, 3.0, 6.0, 2.5);
        assert_almost_eq!(result, 7.5, 0.0);
    }

    #[test]
    fn test_linear_interpolation_same_x() {
        let result = linear_interpolation(1.0, 1.0, 3.0, 6.0, 1.0);
        assert_almost_eq!(result, 4.5, 0.0);
    }

    #[test]
    fn upper_quadratic_root_returns_none_when_no_roots() {
        // y = x^2 + 1
        let result = upper_quadratic_root(1.0, 0.0, 1.0);
        assert!(result.is_none());
    }

    #[test]
    fn upper_quadratic_root_returns_one_root() {
        // y = x^2
        let result = upper_quadratic_root(1.0, 0.0, 0.0).unwrap();
        assert_almost_eq!(result, 0.0, 0.0);
    }

    #[test]
    fn upper_quadratic_root_returns_greater_root() {
        // y = x^2 - 1 has roots of +1 and -1
        let result = upper_quadratic_root(1.0, 0.0, -1.0).unwrap();
        assert_almost_eq!(result, 1.0, 0.0);
    }
}
