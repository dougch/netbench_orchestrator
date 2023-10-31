// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::russula::NextTransitionMsg;
use crate::russula::StateApi;
use async_trait::async_trait;
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};

use crate::russula::error::{RussulaError, RussulaResult};
use crate::russula::protocol::Protocol;

#[derive(Default)]
pub struct NetbenchWorkerProtocol {
    stream: Option<TcpStream>,
    state: NetbenchWorkerState,
    peer_state: NetbenchWorkerState,
}

impl NetbenchWorkerProtocol {
    pub fn new() -> Self {
        NetbenchWorkerProtocol {
            stream: None,
            state: NetbenchWorkerState::Ready,
            peer_state: NetbenchWorkerState::Ready,
        }
    }
}

#[async_trait]
impl Protocol for NetbenchWorkerProtocol {
    type State = NetbenchWorkerState;

    async fn connect(&self, addr: &SocketAddr) -> RussulaResult<TcpStream> {
        let listener = TcpListener::bind(addr).await.unwrap();
        println!("--- Worker listening on: {}", addr);

        let (stream, _local_addr) =
            listener
                .accept()
                .await
                .map_err(|err| RussulaError::Connect {
                    dbg: err.to_string(),
                })?;
        println!("Worker success connection: {addr}");

        Ok(stream)
    }

    async fn set_stream(&mut self, stream: TcpStream) {
        self.stream = Some(stream);
    }

    async fn stream(&self) -> Option<&TcpStream> {
        (&self.stream).into()
    }

    async fn recv_msg(&self, stream: TcpStream) -> RussulaResult<Self::State> {
        stream.readable().await.unwrap();

        let mut buf = Vec::with_capacity(100);
        match stream.try_read_buf(&mut buf) {
            Ok(n) => {
                let msg = NetbenchWorkerState::from_bytes(&buf)?;
                println!("read {} bytes: {:?}", n, &msg);
            }
            Err(ref e) if e.kind() == tokio::io::ErrorKind::WouldBlock => {
                panic!("{}", e)
            }
            Err(e) => panic!("{}", e),
        }

        // TODO
        Ok(self.state)
    }

    async fn send_msg(&self, stream: TcpStream, msg: Self::State) -> RussulaResult<()> {
        stream.writable().await.unwrap();

        stream.try_write(msg.as_bytes()).unwrap();

        Ok(())
    }

    fn state(&self) -> Self::State {
        self.state
    }
    fn peer_state(&self) -> Self::State {
        self.peer_state
    }
}

//  curr_state                self/peer driven       notify peer of curr state          fn to go to next
//
//  Ready(Ip),                Some("ready_next"),    false,                             Running((Ip, TcpStream))
//  Running((Ip, TcpStream)), None,                  true,                              Done((Ip, TcpStream))
//
// A("name",                  Option(MSG_to_next),   Notify_peer_of_transition_to_next, Fn(Self)->Self )
// B("name",                  Option(MSG_to_next),   Notify_peer_of_transition_to_next)
#[derive(Copy, Clone, Debug, Default)]
pub enum NetbenchWorkerState {
    #[default]
    Ready,
    WaitPeerDone,
    Done,
}

impl StateApi for NetbenchWorkerState {
    fn eq(&self, other: Self) -> bool {
        match self {
            NetbenchWorkerState::Ready => matches!(other, NetbenchWorkerState::Ready),
            NetbenchWorkerState::WaitPeerDone => matches!(other, NetbenchWorkerState::WaitPeerDone),
            NetbenchWorkerState::Done => matches!(other, NetbenchWorkerState::Done),
        }
    }

    fn next_transition_msg(&self) -> Option<NextTransitionMsg> {
        match self {
            NetbenchWorkerState::Ready => None,
            NetbenchWorkerState::WaitPeerDone => Some(NextTransitionMsg::PeerDriven(
                "wait_peer_done_next".to_string(),
            )),
            NetbenchWorkerState::Done => None,
        }
    }

    fn next(&mut self) -> Self {
        match self {
            NetbenchWorkerState::Ready => NetbenchWorkerState::WaitPeerDone,
            NetbenchWorkerState::WaitPeerDone => NetbenchWorkerState::Done,
            NetbenchWorkerState::Done => NetbenchWorkerState::Done,
        }
    }

    fn process_msg(&mut self, msg: String) -> &Self {
        if let Some(NextTransitionMsg::PeerDriven(peer_msg)) = self.next_transition_msg() {
            if peer_msg == msg {
                self.next();
            }
        }

        self
    }
}

impl NetbenchWorkerState {
    pub fn as_bytes(&self) -> &'static [u8] {
        match self {
            NetbenchWorkerState::Ready => b"ready",
            NetbenchWorkerState::WaitPeerDone => b"wait_peer_done",
            NetbenchWorkerState::Done => b"done",
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> RussulaResult<Self> {
        let state = match bytes {
            b"ready" => NetbenchWorkerState::Ready,
            b"wait_peer_done" => NetbenchWorkerState::WaitPeerDone,
            b"done" => NetbenchWorkerState::Done,
            bad_msg => {
                return Err(RussulaError::BadMsg {
                    dbg: format!("unrecognized msg {:?}", bad_msg),
                })
            }
        };

        Ok(state)
    }
}

#[derive(Default)]
pub struct NetbenchOrchProtocol {
    stream: Option<TcpStream>,
    state: NetbenchOrchState,
    peer_state: NetbenchOrchState,
}

impl NetbenchOrchProtocol {
    pub fn new() -> Self {
        NetbenchOrchProtocol {
            stream: None,
            state: NetbenchOrchState::Ready,
            peer_state: NetbenchOrchState::Ready,
        }
    }
}

#[async_trait]
impl Protocol for NetbenchOrchProtocol {
    type State = NetbenchOrchState;

    async fn connect(&self, addr: &SocketAddr) -> RussulaResult<TcpStream> {
        println!("--- Coordinator: attempt to connect to worker on: {}", addr);

        let connect = TcpStream::connect(addr)
            .await
            .map_err(|err| RussulaError::Connect {
                dbg: err.to_string(),
            })?;

        Ok(connect)
    }

    async fn set_stream(&mut self, stream: TcpStream) {
        self.stream = Some(stream);
    }

    async fn stream(&self) -> Option<&TcpStream> {
        (&self.stream).into()
    }

    async fn recv_msg(&self, stream: TcpStream) -> RussulaResult<Self::State> {
        stream.readable().await.unwrap();

        let mut buf = Vec::with_capacity(100);
        match stream.try_read_buf(&mut buf) {
            Ok(n) => {
                let msg = NetbenchOrchState::from_bytes(&buf)?;
                println!("read {} bytes: {:?}", n, &msg);
            }
            Err(ref e) if e.kind() == tokio::io::ErrorKind::WouldBlock => {
                panic!("{}", e)
            }
            Err(e) => panic!("{}", e),
        }

        // TODO
        Ok(self.state)
    }

    async fn send_msg(&self, stream: TcpStream, msg: Self::State) -> RussulaResult<()> {
        stream.writable().await.unwrap();

        stream.try_write(msg.as_bytes()).unwrap();

        Ok(())
    }

    fn state(&self) -> Self::State {
        self.state
    }
    fn peer_state(&self) -> Self::State {
        self.peer_state
    }
}

//  curr_state                self/peer driven       notify peer of curr state          fn to go to next
//
//  Ready(Ip),                Some("ready_next"),    false,                             Running((Ip, TcpStream))
//  Running((Ip, TcpStream)), None,                  true,                              Done((Ip, TcpStream))
//
// A("name",                  Option(MSG_to_next),   Notify_peer_of_transition_to_next, Fn(Self)->Self )
// B("name",                  Option(MSG_to_next),   Notify_peer_of_transition_to_next)
#[derive(Copy, Clone, Debug, Default)]
pub enum NetbenchOrchState {
    #[default]
    Ready,
    WaitPeerDone,
    Done,
}

impl StateApi for NetbenchOrchState {
    fn eq(&self, other: Self) -> bool {
        match self {
            NetbenchOrchState::Ready => matches!(other, NetbenchOrchState::Ready),
            NetbenchOrchState::WaitPeerDone => matches!(other, NetbenchOrchState::WaitPeerDone),
            NetbenchOrchState::Done => matches!(other, NetbenchOrchState::Done),
        }
    }

    fn next_transition_msg(&self) -> Option<NextTransitionMsg> {
        match self {
            NetbenchOrchState::Ready => None,
            NetbenchOrchState::WaitPeerDone => Some(NextTransitionMsg::PeerDriven(
                "wait_peer_done_next".to_string(),
            )),
            NetbenchOrchState::Done => None,
        }
    }

    fn next(&mut self) -> Self {
        match self {
            NetbenchOrchState::Ready => NetbenchOrchState::WaitPeerDone,
            NetbenchOrchState::WaitPeerDone => NetbenchOrchState::Done,
            NetbenchOrchState::Done => NetbenchOrchState::Done,
        }
    }

    fn process_msg(&mut self, msg: String) -> &Self {
        if let Some(NextTransitionMsg::PeerDriven(peer_msg)) = self.next_transition_msg() {
            if peer_msg == msg {
                self.next();
            }
        }

        self
    }
}

impl NetbenchOrchState {
    pub fn as_bytes(&self) -> &'static [u8] {
        match self {
            NetbenchOrchState::Ready => b"ready",
            NetbenchOrchState::WaitPeerDone => b"wait_peer_done",
            NetbenchOrchState::Done => b"done",
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> RussulaResult<Self> {
        let state = match bytes {
            b"ready" => NetbenchOrchState::Ready,
            b"wait_peer_done" => NetbenchOrchState::WaitPeerDone,
            b"done" => NetbenchOrchState::Done,
            bad_msg => {
                return Err(RussulaError::BadMsg {
                    dbg: format!("unrecognized msg {:?}", bad_msg),
                })
            }
        };

        Ok(state)
    }
}
