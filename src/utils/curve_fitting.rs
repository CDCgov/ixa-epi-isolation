/// Linear interpolation between two points. Returns the average y value when `x1 == x2`.
#[must_use]
pub fn linear_interpolation(x1: f64, x2: f64, y1: f64, y2: f64, xp: f64) -> f64 {
    // At the tails of a cumulative rate function, we may hit a point where the cumulative rates are
    // the same, but we still need to be able to do interpolation. We just return the average of the
    // y values in this case (x1 == x2 == xp).
    #[allow(clippy::float_cmp)]
    if x1 == x2 {
        return f64::midpoint(y1, y2);
    }
    y1 + (y2 - y1) / (x2 - x1) * (xp - x1)
}
#[cfg(test)]
mod test {
    use ixa::assert_almost_eq;

    use super::linear_interpolation;

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
}
