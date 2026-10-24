#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FallbackPoint {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: f64,
}

pub fn highest(points: &[FallbackPoint]) -> Option<FallbackPoint> {
    points
        .iter()
        .copied()
        .max_by(|a, b| a.altitude.total_cmp(&b.altitude))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_highest_point() {
        let low = FallbackPoint::default();
        let high = FallbackPoint {
            altitude: 120.0,
            ..low
        };
        assert_eq!(highest(&[low, high]), Some(high));
        assert_eq!(highest(&[]), None);
    }
}
