// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::russula::{
    error::{RussulaError, RussulaResult},
    netbench_server_coord::CoordNetbenchServerState,
    protocol::Protocol,
    StateApi, TransitionStep,
};
use async_trait::async_trait;
use core::{fmt::Debug, task::Poll};
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};

#[derive(Copy, Clone, Debug)]
pub enum WorkerNetbenchServerState {
    WaitPeerInit,
    Ready,
    Run,
    Done,
}

#[derive(Clone, Copy)]
pub struct NetbenchWorkerServerProtocol {
    state: WorkerNetbenchServerState,
}

impl NetbenchWorkerServerProtocol {
    pub fn new() -> Self {
        NetbenchWorkerServerProtocol {
            state: WorkerNetbenchServerState::WaitPeerInit,
        }
    }
}

#[async_trait]
impl Protocol for NetbenchWorkerServerProtocol {
    type State = WorkerNetbenchServerState;

    async fn connect(&self, addr: &SocketAddr) -> RussulaResult<TcpStream> {
        let listener = TcpListener::bind(addr).await.unwrap();
        println!("--- Worker listening on: {}", addr);

        let (stream, _local_addr) =
            listener
                .accept()
                .await
                .map_err(|err| RussulaError::NetworkFail {
                    dbg: err.to_string(),
                })?;
        println!("Worker success connection: {addr}");

        Ok(stream)
    }

    async fn run_till_ready(&mut self, stream: &TcpStream) -> RussulaResult<()> {
        self.run_till_state(stream, WorkerNetbenchServerState::Ready)
            .await
    }

    async fn run_till_state(
        &mut self,
        stream: &TcpStream,
        state: Self::State,
    ) -> RussulaResult<()> {
        while !self.state.eq(&state) {
            let prev = self.state;
            self.state.run(stream).await?;
            println!("worker state--------{:?} -> {:?}", prev, self.state);
        }

        Ok(())
    }

    async fn poll_state(
        &mut self,
        stream: &TcpStream,
        state: Self::State,
    ) -> RussulaResult<Poll<()>> {
        if !self.state.eq(&state) {
            let prev = self.state;
            self.state.run(stream).await?;
            println!("worker state--------{:?} -> {:?}", prev, self.state);
        }
        let poll = if self.state.eq(&state) {
            Poll::Ready(())
        } else {
            Poll::Pending
        };
        Ok(poll)
    }

    fn state(&self) -> &Self::State {
        &self.state
    }
}

#[async_trait]
impl StateApi for WorkerNetbenchServerState {
    async fn run(&mut self, stream: &TcpStream) -> RussulaResult<()> {
        match self {
            WorkerNetbenchServerState::WaitPeerInit => {
                self.await_peer_msg(stream).await?;
            }
            WorkerNetbenchServerState::Ready => {
                let res = self.await_peer_msg(stream).await;
                if let Err(RussulaError::NetworkBlocked { dbg: _ }) = res {
                    println!("worker--- no message received.. buffer empty");
                } else {
                    res?
                }
            }
            WorkerNetbenchServerState::Run => self.transition_next(),
            WorkerNetbenchServerState::Done => self.transition_next(),
        }

        Ok(())
    }

    fn eq(&self, other: &Self) -> bool {
        match self {
            WorkerNetbenchServerState::WaitPeerInit => {
                matches!(other, WorkerNetbenchServerState::WaitPeerInit)
            }
            WorkerNetbenchServerState::Ready => {
                matches!(other, WorkerNetbenchServerState::Ready)
            }
            WorkerNetbenchServerState::Run => {
                matches!(other, WorkerNetbenchServerState::Run)
            }
            WorkerNetbenchServerState::Done => {
                matches!(other, WorkerNetbenchServerState::Done)
            }
        }
    }

    fn transition_step(&self) -> TransitionStep {
        match self {
            WorkerNetbenchServerState::WaitPeerInit => {
                TransitionStep::AwaitPeerState(CoordNetbenchServerState::CheckPeer.as_bytes())
            }
            WorkerNetbenchServerState::Ready => {
                TransitionStep::AwaitPeerState(CoordNetbenchServerState::RunPeer.as_bytes())
            }
            WorkerNetbenchServerState::Run => {
                TransitionStep::AwaitPeerState(CoordNetbenchServerState::KillPeer.as_bytes())
            }
            WorkerNetbenchServerState::Done => TransitionStep::Finished,
        }
    }

    fn transition_next(&mut self) {
        println!("worker------------- moving to next state {:?}", self);
        *self = self.next_state();
    }

    fn next_state(&self) -> Self {
        match self {
            WorkerNetbenchServerState::WaitPeerInit => WorkerNetbenchServerState::Ready,
            WorkerNetbenchServerState::Ready => WorkerNetbenchServerState::Run,
            WorkerNetbenchServerState::Run => WorkerNetbenchServerState::Done,
            WorkerNetbenchServerState::Done => WorkerNetbenchServerState::Done,
        }
    }

    fn as_bytes(&self) -> &'static [u8] {
        match self {
            WorkerNetbenchServerState::WaitPeerInit => b"server_wait_coord_init",
            WorkerNetbenchServerState::Ready => b"server_ready",
            WorkerNetbenchServerState::Run => b"server_wait_peer_done",
            WorkerNetbenchServerState::Done => b"server_done",
        }
    }

    fn from_bytes(bytes: &[u8]) -> RussulaResult<Self> {
        let state = match bytes {
            b"server_wait_coord_init" => WorkerNetbenchServerState::WaitPeerInit,
            b"server_ready" => WorkerNetbenchServerState::Ready,
            b"server_wait_peer_done" => WorkerNetbenchServerState::Run,
            b"server_done" => WorkerNetbenchServerState::Done,
            bad_msg => {
                return Err(RussulaError::BadMsg {
                    dbg: format!("unrecognized msg {:?}", std::str::from_utf8(bad_msg)),
                })
            }
        };

        Ok(state)
    }
}

#[cfg(test)]
mod tests {

    #[tokio::test]
    async fn netbench_state() {}
}
