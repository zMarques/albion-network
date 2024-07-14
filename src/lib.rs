use photon_decode::{Message, Photon};
use pnet::{
    datalink::{self, Channel, Config, NetworkInterface},
    packet::{
        ethernet::{EtherTypes::Ipv4, EthernetPacket},
        ip::IpNextHeaderProtocols,
        ipv4::Ipv4Packet,
        Packet,
    },
};
use std::{
    sync::mpsc::{channel, Receiver, Sender},
    sync::{Arc, Mutex},
    thread,
};

pub type UdpPacketPayload = Vec<u8>;

const GAME_PORT: u16 = 5056;
const MAX_PACKET_SIZE: usize = 1600;

pub fn listen_packets<F>(cb: F)
where
    F: Fn(Message) + Send + Sync + 'static,
{
    let interfaces = get_network_interfaces().unwrap();
    let (tx, rx): (Sender<UdpPacketPayload>, Receiver<UdpPacketPayload>) = channel();
    receive(interfaces, tx);
    decode(rx, cb);
}

fn decode<F>(rx: Receiver<UdpPacketPayload>, cb: F)
where
    F: Fn(Message) + Send + Sync + 'static,
{
    let mut photon = Photon::new();
    thread::spawn(move || {
        for udp_packet in rx {
            for message in photon.decode(&udp_packet) {
                cb(message)
            }
        }
    });
}

fn get_network_interfaces() -> Result<Vec<NetworkInterface>, &'static str> {
    Ok(datalink::interfaces()
        .into_iter()
        .filter(|i| !i.is_loopback())
        .collect::<Vec<NetworkInterface>>())
}

fn receive(interfaces: Vec<NetworkInterface>, tx: Sender<UdpPacketPayload>) {
    let shared_tx = Arc::new(Mutex::new(tx));

    let config = Config {
        write_buffer_size: MAX_PACKET_SIZE,
        read_buffer_size: MAX_PACKET_SIZE,
        ..Default::default()
    };

    for interface in interfaces {
        let tx = shared_tx.clone();

        let mut rx = match datalink::channel(&interface, config) {
            Ok(Channel::Ethernet(_, rx)) => rx,
            _ => continue,
        };

        thread::spawn(move || loop {
            match rx.next() {
                Ok(packet) => {
                    let ethernet = &EthernetPacket::new(packet).unwrap();
                    if ethernet.get_ethertype() == Ipv4 {
                        let header = Ipv4Packet::new(ethernet.payload()).unwrap();
                        let next_header = header.get_next_level_protocol();

                        if next_header == IpNextHeaderProtocols::Udp {
                            let udp = pnet::packet::udp::UdpPacket::new(header.payload()).unwrap();
                            let udp_source = udp.get_source();
                            let udp_destination = udp.get_destination();

                            if udp_source == GAME_PORT || udp_destination == GAME_PORT {
                                let tx = tx.lock().unwrap();
                                tx.send(Vec::from(udp.payload())).unwrap();
                            }
                        }
                    }
                }
                Err(_) => continue,
            };
        });
    }
}
