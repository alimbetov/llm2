use crate::error::AstraError;

pub fn l2_normalize(mut vector: Vec<f32>, expected: usize) -> Result<Vec<f32>, AstraError> {
    if vector.len() != expected {
        return Err(AstraError::Internal(format!(
            "dense dimension {}, expected {expected}",
            vector.len()
        )));
    }
    if vector.iter().any(|v| !v.is_finite()) {
        return Err(AstraError::Internal(
            "dense vector contains NaN/Infinity".into(),
        ));
    }
    let norm = vector
        .iter()
        .map(|v| (*v as f64).powi(2))
        .sum::<f64>()
        .sqrt();
    if norm <= f64::EPSILON {
        return Err(AstraError::Internal("zero dense vector".into()));
    }
    for v in &mut vector {
        *v = (*v as f64 / norm) as f32;
    }
    Ok(vector)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalizes() {
        let v = l2_normalize(vec![3.0, 4.0], 2).unwrap();
        assert!((v[0] - 0.6).abs() < 1e-6);
    }
}
