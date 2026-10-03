//! Speed curve presets: how the speed of a clip changes from its start (position 0) to its end (position 1).

/// A ready-made speed curve: points `(position along the clip 0..=1, speed)` joined by straight lines.
#[derive(Clone, Debug, PartialEq)]
pub struct SpeedCurve {
    pub name: &'static str,
    pub points: Vec<(f32, f32)>,
}

fn c(name: &'static str, points: &[(f32, f32)]) -> SpeedCurve {
    SpeedCurve {
        name,
        points: points.to_vec(),
    }
}

/// The built-in curves. (A custom curve is made by dragging points on the curve editor.)
pub fn speed_curves() -> Vec<SpeedCurve> {
    vec![
        c(
            "Montage",
            &[(0.0, 0.6), (0.25, 2.5), (0.5, 0.6), (0.75, 2.5), (1.0, 0.6)],
        ),
        c("Hero", &[(0.0, 1.4), (0.35, 0.3), (0.65, 0.3), (1.0, 2.0)]),
        c(
            "Bullet",
            &[
                (0.0, 1.0),
                (0.25, 5.0),
                (0.5, 0.15),
                (0.75, 5.0),
                (1.0, 1.0),
            ],
        ),
        c(
            "Jump cut",
            &[
                (0.0, 1.0),
                (0.33, 1.0),
                (0.34, 3.0),
                (0.66, 3.0),
                (0.67, 0.6),
                (1.0, 0.6),
            ],
        ),
        c("Flash in", &[(0.0, 8.0), (0.25, 1.0), (1.0, 1.0)]),
        c("Flash out", &[(0.0, 1.0), (0.75, 1.0), (1.0, 8.0)]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_curve_runs_from_the_start_to_the_end_in_order_with_a_positive_average() {
        for curve in speed_curves() {
            let p = &curve.points;
            assert_eq!(p.first().unwrap().0, 0.0, "{}", curve.name);
            assert_eq!(p.last().unwrap().0, 1.0, "{}", curve.name);
            assert!(
                p.windows(2).all(|w| w[0].0 < w[1].0),
                "{} is in order",
                curve.name
            );
            assert!(p.iter().all(|(_, s)| *s > 0.0 && *s <= 100.0));
        }
    }
}
