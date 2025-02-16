#[must_use]
pub fn linear_interpolation(x1: f64, x2: f64, y1: f64, y2: f64, x: f64) -> f64 {
    y1 + (y2 - y1) * (x - x1) / (x2 - x1)
}

#[cfg(test)]
mod test {
    #[test]
    fn test_linear_interpolation() {}
}
