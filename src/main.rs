use std::{net, time::Duration};

use rvoip_sip::{EndpointAudioSender, Endpoint, EndpointAudioFrame, EndpointProfile};
use tokio::time::interval;

async fn play_wav_into_call(tx: EndpointAudioSender, path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();

    if spec.sample_format != hound::SampleFormat::Int || spec.bits_per_sample != 16 {
        return Err("WAV must be 16-bit PCM for this example".into());
    }
    if spec.sample_rate != 8000 {
        return Err(format!("WAV must be 8kHz (got {}Hz) — resample first", spec.sample_rate).into());
    }

    let sample_rate = spec.sample_rate as usize;
    let channels = spec.channels as usize;

    let frame_ms = 20;
    let samples_per_frame_per_channel = sample_rate * frame_ms / 1_000;
    let samples_per_frame = samples_per_frame_per_channel * channels;

    let samples: Vec<i16> = reader.samples::<i16>().collect::<Result<_, _>>()?;

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

    // Sending frames in real-time
    let mut rtp_ts: u32 = rand::random::<u32>();
    let mut ticker = interval(Duration::from_millis(frame_ms as u64));
    let mut pos = 0usize;
    while pos < mono_samples.len() {
        ticker.tick().await;
        let end = (pos + samples_per_frame_per_channel).min(mono_samples.len());
        let frame_samples = &mono_samples[pos..end];

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


#[tokio::main]
async fn main() -> rvoip_sip::Result<()> {

    // SETUP //////////////////////////////////////

    let bind_addr: net::SocketAddr = "192.168.129.60:5070".parse().unwrap();
    let doorbell = Endpoint::builder()
        .name("doorbell")
        .bind_addr(bind_addr)
        .advertised_addr(bind_addr)
        .profile(EndpointProfile::Local)
        .build()
        .await?;

    // CALLING TARGET /////////////////////////////

    let target = "sip:elliot@192.168.129.12:5060";
    let call = doorbell
        .call_and_wait(target, Some(Duration::from_secs(10)))
        .await?;
    println!("[doorbell] connected as {}", call.id());

    // SENDING WAVEFORM ///////////////////////////

    let audio_handle = call.audio().await?;
    let (tx, _) = audio_handle.split();

    if let Err(e) = play_wav_into_call(tx, "res/test.wav").await {
        eprintln!("playback error: {:?}", e);
    }

    // SENDING DTMF SIGNALS ///////////////////////

    tokio::time::sleep(Duration::from_secs(1)).await;
    call.send_dtmf('1').await?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    call.send_dtmf('2').await?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    call.send_dtmf('*').await?;

    tokio::time::sleep(Duration::from_secs(5)).await;

    // SHUTDOWN ///////////////////////////////////

    call.hangup_and_wait(None).await?;

    doorbell.shutdown().await?;

    Ok(())
}
