// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::russula::protocol::{RussulaPeer, SockProtocol};
use core::task::Poll;
use std::{collections::BTreeSet, net::SocketAddr};

mod error;
mod netbench_server_coord;
mod netbench_server_worker;
mod network_utils;
mod protocol;
mod state_action;
mod wip_netbench_server;

use error::{RussulaError, RussulaResult};
use protocol::Protocol;

use self::protocol::{StateApi, TransitionStep};

// TODO
// - make state transitions nicer..
//
// D- len for msg
// D- r.transition_step // what is the next step one should take
// D- r.poll_state // take steps to go to next step if possible
// - should poll current step until all peers are on next step
//   - need api to ask peer state and track peer state
//
// - look at NTP for synchronization: start_at(time)
// - handle coord retry on connect
// D- move connect to protocol impl
// https://statecharts.dev/
// halting problem https://en.wikipedia.org/wiki/Halting_problem

pub struct Russula<P: Protocol> {
    peer_list: Vec<RussulaPeer<P>>,
}

impl<P: Protocol + Send> Russula<P> {
    pub async fn run_till_ready(&mut self) {
        for peer in self.peer_list.iter_mut() {
            peer.protocol.run_till_ready(&peer.stream).await.unwrap();
        }
    }

    pub async fn poll_next(&mut self) -> Poll<()> {
        for peer in self.peer_list.iter_mut() {
            // poll till state and break if Pending
            let poll = peer.protocol.poll_next(&peer.stream).await.unwrap();
            if poll.is_pending() {
                return Poll::Pending;
            }
        }
        Poll::Ready(())
    }

    pub async fn check_self_state(&self, state: P::State) -> RussulaResult<bool> {
        let mut matches = true;
        for peer in self.peer_list.iter() {
            let protocol_state = peer.protocol.state();
            matches &= state.eq(protocol_state);
            // println!("{:?} {:?} {}", protocol_state, state, matches);
        }
        Ok(matches)
    }

    pub fn transition_step(&mut self) -> Vec<TransitionStep> {
        let mut steps = Vec::new();
        for peer in self.peer_list.iter() {
            let step = peer.protocol.state().transition_step();
            steps.push(step);
        }
        steps
    }
}

pub struct RussulaBuilder<P: Protocol> {
    peer_list: Vec<SockProtocol<P>>,
}

impl<P: Protocol> RussulaBuilder<P> {
    pub fn new(addr: BTreeSet<SocketAddr>, protocol: P) -> Self {
        let mut map = Vec::new();
        addr.into_iter().for_each(|addr| {
            map.push((addr, protocol.clone()));
        });
        Self { peer_list: map }
    }

    pub async fn build(self) -> RussulaResult<Russula<P>> {
        let mut stream_protocol_list = Vec::new();
        for (addr, protocol) in self.peer_list.into_iter() {
            let stream = protocol.connect(&addr).await?;
            println!("Coordinator: successfully connected to {}", addr);
            stream_protocol_list.push(RussulaPeer {
                addr,
                stream,
                protocol,
            });
        }

        Ok(Russula {
            peer_list: stream_protocol_list,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::russula::{
        netbench_server_coord::{CoordNetbenchServerState, NetbenchCoordServerProtocol},
        netbench_server_worker::{NetbenchWorkerServerProtocol, WorkerNetbenchServerState},
    };
    use core::time::Duration;
    use std::str::FromStr;

    #[tokio::test]
    async fn russula_netbench() {
        let w1_sock = SocketAddr::from_str("127.0.0.1:8991").unwrap();
        let w2_sock = SocketAddr::from_str("127.0.0.1:8993").unwrap();
        let worker_list = [w2_sock];

        let w1 = tokio::spawn(async move {
            let worker = RussulaBuilder::new(
                BTreeSet::from_iter([w1_sock]),
                NetbenchWorkerServerProtocol::new(w1_sock.port()),
            );
            let mut worker = worker.build().await.unwrap();
            worker.run_till_ready().await;
            worker
        });
        let w2 = tokio::spawn(async move {
            let worker = RussulaBuilder::new(
                BTreeSet::from_iter([w2_sock]),
                NetbenchWorkerServerProtocol::new(w2_sock.port()),
            );
            let mut worker = worker.build().await.unwrap();

            // worker.run_till_ready().await;

            while !worker
                .check_self_state(WorkerNetbenchServerState::Run)
                .await
                .unwrap()
            {
                println!("run--o--o-o-o-oo-----ooooooooo---------o");
                let _ = worker.poll_next().await;
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            worker
        });

        let c1 = tokio::spawn(async move {
            let addr = BTreeSet::from_iter(worker_list);
            let coord = RussulaBuilder::new(addr, NetbenchCoordServerProtocol::new());
            let mut coord = coord.build().await.unwrap();
            coord.run_till_ready().await;
            coord
        });

        let join = tokio::join!(c1);
        let mut coord = join.0.unwrap();
        // let mut worker1 = join.1.unwrap();

        println!("\nSTEP 1 --------------- : confirm current ready state");
        // we are already in the Ready state
        {
            // assert!(worker1
            //     .check_self_state(WorkerNetbenchServerState::Ready)
            //     .await
            //     .unwrap());
            assert!(coord
                .check_self_state(CoordNetbenchServerState::Ready)
                .await
                .unwrap());
        }

        println!("\nSTEP 2 --------------- : check next transition step");
        // we are pendng next state on UserDriven action on the coord
        {
            let _s = CoordNetbenchServerState::RunPeer.as_bytes();
            // assert!(matches!(
            //     worker1.transition_step()[0],
            //     TransitionStep::AwaitPeer(_s)
            // ));
            assert!(matches!(
                coord.transition_step()[0],
                TransitionStep::UserDriven
            ));
        }

        println!("\nSTEP 3 --------------- : confirm AwaitPeerMsg cant self transition");
        {
            // assert!(worker1.poll_next().await.is_pending(),);
            // assert!(worker1
            //     .check_self_state(WorkerNetbenchServerState::Ready)
            //     .await
            //     .unwrap());
        }

        println!("\nSTEP 4 --------------- : poll next coord step");
        // move coord forward
        {
            assert!(coord.poll_next().await.is_ready());
            assert!(coord
                .check_self_state(CoordNetbenchServerState::RunPeer)
                .await
                .unwrap());
        }

        println!("\nSTEP 5 --------------- : poll worker next step");
        {
            // assert!(worker1
            //     .check_self_state(WorkerNetbenchServerState::Ready)
            //     .await
            //     .unwrap());
            // assert!(worker1.poll_next().await.is_pending());
            // while !worker1
            //     .check_self_state(WorkerNetbenchServerState::Run)
            //     .await
            //     .unwrap()
            // {
            //     println!("run--o--o-o-o-oo-----ooooooooo---------o");
            //     let _ = worker1.poll_next().await;
            //     tokio::time::sleep(Duration::from_secs(1)).await;
            // }
            // assert!(worker1
            //     .check_self_state(WorkerNetbenchServerState::Run)
            //     .await
            //     .unwrap());
        }

        println!("\nSTEP 6 --------------- : poll coord and kill peer");
        {
            // assert!(coord.poll_next().await.is_ready());
            // assert!(coord
            //     .check_self_state(CoordNetbenchServerState::KillPeer)
            //     .await
            //     .unwrap());
        }

        let join = tokio::join!(w2);
        // let mut coord = join.0.unwrap();
        let mut worker2 = join.0.unwrap();
        assert!(worker2
            .check_self_state(WorkerNetbenchServerState::Run)
            .await
            .unwrap());

        assert!(22 == 20, "\n\n\nSUCCESS ---------------- INTENTIONAL FAIL");
    }
}
