// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::NetbenchDriver;
use super::{send_command, Step};
use crate::{state::STATE, Scenario};
use aws_sdk_ssm::operation::send_command::SendCommandOutput;
use std::net::{IpAddr, SocketAddr};
use tracing::debug;

pub async fn copy_netbench_data(
    ssm_client: &aws_sdk_ssm::Client,
    instance_ids: Vec<String>,
    unique_id: &str,
    scenario: &Scenario,
) -> SendCommandOutput {
    send_command(
        vec![Step::RunRussula],
        Step::RunNetbench,
        "client",
        "run_client_netbench",
        ssm_client,
        instance_ids,
        vec![
            "cd netbench_orchestrator",
            format!(
                "aws s3 cp client.json {}/results/{}/s2n-quic/",
                STATE.s3_path(unique_id),
                scenario.file_stem()
            )
            .as_str(),
        ]
        .into_iter()
        .map(String::from)
        .collect(),
    )
    .await
    .expect("Timed out")
}

pub async fn run_russula_worker(
    ssm_client: &aws_sdk_ssm::Client,
    instance_ids: Vec<String>,
    server_ips: &Vec<IpAddr>,
    driver: &NetbenchDriver,
    scenario: &Scenario,
) -> SendCommandOutput {
    let netbench_server_addr = server_ips
        .iter()
        .map(|ip| SocketAddr::new(*ip, STATE.netbench_port).to_string())
        .reduce(|mut accum, item| {
            accum.push(' ');
            accum.push_str(&item);
            accum
        })
        .unwrap();

    let netbench_cmd =
        format!("env RUST_LOG=debug ./target/debug/russula_cli netbench-client-worker --russula-port {} --driver {} --scenario {} --netbench-servers {netbench_server_addr}",
            STATE.russula_port, driver.driver_name, scenario.name);
    debug!("{}", netbench_cmd);

    send_command(
        vec![Step::BuildDriver("".to_string()), Step::BuildRussula],
        Step::RunRussula,
        "client",
        "run_client_russula",
        ssm_client,
        instance_ids,
        vec!["cd netbench_orchestrator", netbench_cmd.as_str()]
            .into_iter()
            .map(String::from)
            .collect(),
    )
    .await
    .expect("Timed out")
}
