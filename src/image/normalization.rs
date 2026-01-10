#[inline]
#[must_use]
pub fn find_min_max(values: &[u32]) -> (f32, f32) {
    values
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), &val| {
            let val_f32 = val as f32;
            (min.min(val_f32), max.max(val_f32))
        })
}

#[inline]
#[must_use]
pub fn normalize_u32_to_u8(value: u32, min: f32, range: f32) -> u8 {
    let value_f32 = value as f32;
    let normalized = (value_f32 - min) / range;
    (normalized * 255.0_f32) as u8
}
