#[must_use]
pub fn linear_interpolation(x1: f64, x2: f64, y1: f64, y2: f64, xp: f64) -> f64 {
    #[allow(clippy::float_cmp)]
    if x1 == x2 {
        return (y1 + y2) / 2.0;
    }
    y1 + (y2 - y1) / (x2 - x1) * (xp - x1)
}

#[cfg(test)]
mod test {
    use statrs::assert_almost_eq;

    use super::linear_interpolation;

    #[test]
    fn test_linear_interpolation_simple() {
        let result = linear_interpolation(1.0, 2.0, 3.0, 6.0, 1.25);
        assert_almost_eq!(result, 3.75, 0.0);
    }

    #[test]
    fn test_linear_exterpolation_simple() {
        let result = linear_interpolation(1.0, 2.0, 3.0, 6.0, 2.5);
        assert_almost_eq!(result, 7.5, 0.0);
    }

    #[test]
    fn test_linear_interpolation_same_x() {
        let result = linear_interpolation(1.0, 1.0, 3.0, 6.0, 1.0);
        assert_almost_eq!(result, 4.5, 0.0);
    }
}
