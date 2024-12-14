use ixa::Context;

/// Provides mathematical utility functions that often arise in numerical simulation.
/// These are generic mathematical tools, and most of them do not require `Context` itself.
/// However, the methods are grouped as a trait extension on `Context` to keep the code organized
/// and maintain our convention of having modules contribute trait extensions to `Context`.
/// In the future, there may be utility functions that require `Context` itself.
pub trait ContextUtilsExt {
    /// Linearly interpolate between two points. Takes the x values of the two points (`x1`, `x2`),
    /// the y values of the two points (`y1`, `y2`), and the x value at which to interpolate the y value (`xp`).
    fn linear_interpolation(&self, x1: f64, x2: f64, y1: f64, y2: f64, xp: f64) -> f64;
}

impl ContextUtilsExt for Context {
    fn linear_interpolation(&self, x1: f64, x2: f64, y1: f64, y2: f64, xp: f64) -> f64 {
        assert!(
            xp >= x1 && xp <= x2,
            "Interpolation point must be within the range of the x values."
        );
        // At the tails of the CDF, the CDF moves very slowly, so with numerical precision, x2 may equal x1.
        #[allow(clippy::float_cmp)]
        if x2 == x1 {
            // In this case, interpolation is useless, so just average the y values.
            return (y1 + y2) / 2.0;
        }
        (y2 - y1) / (x2 - x1) * (xp - x1) + y1
    }
}

#[cfg(test)]
mod test {
    use ixa::Context;

    use super::ContextUtilsExt;

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_linear_interpolation_simple() {
        let context = Context::new();
        let result = context.linear_interpolation(1.0, 2.0, 3.0, 6.0, 1.25);
        assert_eq!(result, 3.75);
    }

    #[test]
    #[should_panic(expected = "Interpolation point must be within the range of the x values.")]
    fn test_linear_interpolation_panic_below() {
        let context = Context::new();
        context.linear_interpolation(1.0, 2.0, 3.0, 6.0, 0.25);
    }

    #[test]
    #[should_panic(expected = "Interpolation point must be within the range of the x values.")]
    fn test_linear_interpolation_panic_above() {
        let context = Context::new();
        context.linear_interpolation(1.0, 2.0, 3.0, 6.0, 2.25);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_linear_interpolation_x1_is_x2() {
        let context = Context::new();
        let result = context.linear_interpolation(1.0, 1.0, 3.0, 6.0, 1.0);
        assert_eq!(result, 4.5);
    }
}
