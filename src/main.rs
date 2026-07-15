use std::{net, time::Duration};

use rvoip_sip::{Config, Endpoint, EndpointProfile, SipTraceConfig};


#[tokio::main]
async fn main() -> rvoip_sip::Result<()> {

    // SETUP //////////////////////////////////////

    let bind_addr: net::SocketAddr = "192.168.129.60:5070".parse().unwrap();
    let mut doorbell = Endpoint::builder()
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
