use std::{error::Error, fmt::Display, time::Duration};

use hound::WavReader;
use rvoip_sip::{EndpointAudioFrame, EndpointAudioSender};
use tokio::time::interval;

const FRAME_MS: usize = 20;

type WavRdr = WavReader<std::io::BufReader<std::fs::File>>;

#[derive(Debug)]
pub struct MonoAudioSamples8Khz {
    data: Vec<i16>,
    sample_rate: usize,
}

impl AsRef<[i16]> for MonoAudioSamples8Khz {
    fn as_ref(&self) -> &[i16] {
        &self.data
    }
}

impl TryFrom<WavRdr> for MonoAudioSamples8Khz {
    type Error = AudioSamplesError;

    fn try_from(mut reader: WavRdr) -> Result<Self, Self::Error> {
        let spec = reader.spec();

        if spec.sample_format != hound::SampleFormat::Int {
            return Err(AudioSamplesError::UnsupportedSampleFormat);
        };
        if spec.bits_per_sample != 16 {
            return Err(AudioSamplesError::UnsupportedBitDepth(spec.bits_per_sample));
        };
        if spec.sample_rate != 8_000 {
            return Err(AudioSamplesError::WrongSampleRate(spec.sample_rate));
        };

        let sample_rate = spec.sample_rate as usize;
        let channels = spec.channels as usize;

        let samples: Vec<i16> = reader
            .samples::<i16>()
            .collect::<Result<_, _>>()
            .map_err(AudioSamplesError::SamplingFailed)?;

        // Turn multi-channel into mono.
        let mono_samples = if channels == 1 {
            samples
        } else {
            let frames = samples.len() / channels;
            let mut out = Vec::with_capacity(frames);
            for frame_idx in 0..frames {
                let mut acc: i32 = 0;
                for ch in 0..channels {
                    acc += samples[frame_idx * channels + ch] as i32;
                }
                out.push((acc / (channels as i32)) as i16);
            }
            out
        };

        Ok(Self {
            data: mono_samples,
            sample_rate,
        })
    }
}

#[derive(Debug)]
pub enum AudioSamplesError {
    UnsupportedSampleFormat,
    UnsupportedBitDepth(u16),
    WrongSampleRate(u32),
    SamplingFailed(hound::Error),
}

impl Display for AudioSamplesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Error for AudioSamplesError {}

pub async fn play_wav_into_call(
    tx: EndpointAudioSender,
    path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let reader = hound::WavReader::open(path)?;
    let samples = MonoAudioSamples8Khz::try_from(reader)?;

    play_sample_into_call(tx, samples).await?;

    Ok(())
}

pub async fn play_sample_into_call(
    tx: EndpointAudioSender,
    MonoAudioSamples8Khz {
        data: samples,
        sample_rate,
    }: MonoAudioSamples8Khz,
) -> Result<(), Box<dyn std::error::Error>> {
    let samples_per_frame_per_channel = (sample_rate * FRAME_MS / 1_000) as usize;

    // Sending frames in real-time
    let mut rtp_ts: u32 = rand::random::<u32>();
    let mut ticker = interval(Duration::from_millis(FRAME_MS as u64));
    let mut pos = 0usize;

    while pos < samples.len() {
        ticker.tick().await;
        let end = (pos + samples_per_frame_per_channel).min(samples.len());
        let frame_samples = &samples[pos..end];

        let mut frame_buf: Vec<i16> = Vec::with_capacity(samples_per_frame_per_channel);
        frame_buf.extend_from_slice(frame_samples);
        if frame_buf.len() < samples_per_frame_per_channel {
            frame_buf.resize(samples_per_frame_per_channel, 0);
        }

        let frame = EndpointAudioFrame::pcmu_sized_mono_8khz(frame_buf, rtp_ts);
        if let Err(e) = tx.send(frame).await {
            eprintln!("audio send error: {:?}", e);
            break;
        }

        rtp_ts = rtp_ts.wrapping_add(samples_per_frame_per_channel as u32);
        pos = end;
    }

    Ok(())
}
