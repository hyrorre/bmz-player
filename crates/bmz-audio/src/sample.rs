#[derive(Debug, Clone)]
pub struct DecodedSample {
    pub channels: u16,
    pub sample_rate: u32,
    pub frames: Vec<f32>,
}

#[derive(Debug, Default)]
pub struct SampleBank;
