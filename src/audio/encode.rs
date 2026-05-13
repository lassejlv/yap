use anyhow::Result;
use hound::{SampleFormat, WavSpec, WavWriter};
use std::io::Cursor;

pub const TARGET_SAMPLE_RATE: u32 = 16_000;

pub fn wav_bytes(samples: &[i16]) -> Result<Vec<u8>> {
    let spec = WavSpec {
        channels: 1,
        sample_rate: TARGET_SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };

    let mut buf = Cursor::new(Vec::with_capacity(samples.len() * 2 + 44));
    {
        let mut writer = WavWriter::new(&mut buf, spec)?;
        for s in samples {
            writer.write_sample(*s)?;
        }
        writer.finalize()?;
    }
    Ok(buf.into_inner())
}
