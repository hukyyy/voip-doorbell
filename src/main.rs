use std::{net, time::Duration};

use rvoip_sip::{Endpoint, EndpointProfile, Event};
use voip_doorbell::play_wav_into_call;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();
    let doorbell_addr = std::env::var("DOORBELL_IP").expect("Please define DOORBELL_IP");
    let gateway_target = std::env::var("GATEWAY_TARGET").expect("Please define GATEWAY_TARGET");

    // SETUP //////////////////////////////////////

    let bind_addr: net::SocketAddr = doorbell_addr.parse().unwrap();
    let doorbell = Endpoint::builder()
        .name("doorbell")
        .bind_addr(bind_addr)
        .advertised_addr(bind_addr)
        .profile(EndpointProfile::Local)
        .build()
        .await?;

    // CALLING TARGET /////////////////////////////

    let call = doorbell
        .call_and_wait(&gateway_target, Some(Duration::from_secs(20)))
        .await?;
    println!("[doorbell] connected as {}", call.id());

    // SENDING WAVEFORM ///////////////////////////

    let audio_handle = call.audio().await?;
    let (tx, _rx) = audio_handle.split();
    // Do not drop _rx to be able to detect DTMF tones.

    let call_events = call.clone();
    let dtmf_task = tokio::spawn(async move {
        if let Ok(mut event_rx) = call_events.as_session_handle().events().await {
            while let Some(ev) = event_rx.next().await {
                if let Event::DtmfReceived { digit, .. } = ev {
                    println!("[doorbell] RECEIVED DTMF: {digit}");
                    if digit == '#' {
                        break;
                    }
                }
            }
        }
    });

    // play music, then STAY on the call so you can press keys on the PAP2 handset
    tokio::spawn(async move {
        let _ = play_wav_into_call(tx, "res/test.wav").await;
    });

    println!("[doorbell] call is up — press keys on the PAP2 handset now");
    dtmf_task.await?; // Wait for # press.
    println!("[doorbell] Shutting down.");

    call.hangup_and_wait(None).await?;
    doorbell.shutdown().await?;

    Ok(())
}
