// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::{send_command, Step};
use crate::{poll_ssm_results, state::STATE, NetbenchDriver};
use aws_sdk_ssm::operation::send_command::SendCommandOutput;
use core::time::Duration;
use indicatif::{ProgressBar, ProgressStyle};

fn get_progress_bar(cmds: &Vec<SendCommandOutput>) -> ProgressBar {
    // TODO use multi-progress bar https://github.com/console-rs/indicatif/blob/main/examples/multi.rs
    let total_tasks = cmds.len() as u64;
    let bar = ProgressBar::new(total_tasks);
    let style = ProgressStyle::with_template(
        "{spinner} [{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}",
    )
    .unwrap()
    .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ");
    bar.set_style(style);
    bar.enable_steady_tick(Duration::from_secs(1));
    bar
}

pub async fn wait_complete(
    host_group: &str,
    ssm_client: &aws_sdk_ssm::Client,
    cmds: Vec<SendCommandOutput>,
) {
    let total_tasks = cmds.len() as u64;
    let bar = get_progress_bar(&cmds);
    loop {
        let mut completed_tasks = 0;
        for cmd in cmds.iter() {
            let _comment = cmd
                .command()
                .unwrap()
                .comment()
                .map(|s| s.to_string())
                .unwrap();
            let cmd_id = cmd.command().unwrap().command_id().unwrap();
            let poll_cmd = poll_ssm_results(host_group, ssm_client, cmd_id)
                .await
                .unwrap();
            if poll_cmd.is_ready() {
                completed_tasks += 1;
            }
        }

        bar.set_position(completed_tasks);
        bar.set_message(host_group.to_string());

        if total_tasks == completed_tasks {
            bar.finish();
            break;
        }
        tokio::time::sleep(STATE.poll_delay_ssm).await;
    }
}

pub async fn collect_config_cmds(
    host_group: &str,
    ssm_client: &aws_sdk_ssm::Client,
    instance_ids: Vec<String>,
    netbench_drivers: &[&NetbenchDriver],
    unique_id: &str,
) -> Vec<SendCommandOutput> {
    // configure and build
    let install_deps =
        install_deps_cmd(host_group, ssm_client, instance_ids.clone(), unique_id).await;

    let mut build_drivers = Vec::new();
    for driver in netbench_drivers {
        let build_driver_cmd = build_netbench_driver_cmd(
            host_group,
            driver,
            ssm_client,
            instance_ids.clone(),
            unique_id,
        )
        .await;
        build_drivers.push(build_driver_cmd);
    }
    let build_russula = build_russula_cmd(host_group, ssm_client, instance_ids.clone()).await;

    vec![install_deps, build_russula]
        .into_iter()
        .chain(build_drivers)
        .collect()
}

async fn install_deps_cmd(
    host_group: &str,
    ssm_client: &aws_sdk_ssm::Client,
    instance_ids: Vec<String>,
    unique_id: &str,
) -> SendCommandOutput {
    send_command(vec![], Step::Configure, host_group, &format!("configure_host_{}", host_group) ,ssm_client, instance_ids, vec![
        // set instances to shutdown after 1 hour
        format!("shutdown -P +{}", STATE.shutdown_min),
        "mkdir -p /home/ec2-user/bin".to_string(),

        format!("echo ec2 up > /home/ec2-user/index.html && aws s3 cp /home/ec2-user/index.html {}/{}-step-1", STATE.s3_path(unique_id), host_group),
        "yum upgrade -y".to_string(),
        format!("echo yum upgrade finished > /home/ec2-user/index.html && aws s3 cp /home/ec2-user/index.html {}/{}-step-2", STATE.s3_path(unique_id), host_group),
        format!("timeout 5m bash -c 'until yum install cargo cmake git perl openssl-devel bpftrace perf tree -y; do sleep 10; done' || (echo yum failed > /home/ec2-user/index.html; aws s3 cp /home/ec2-user/index.html {}/{}-step-3; exit 1)", STATE.s3_path(unique_id), host_group),
        format!("echo yum finished > /home/ec2-user/index.html && aws s3 cp /home/ec2-user/index.html {}/{}-step-3", STATE.s3_path(unique_id), host_group),
        // rust
        "runuser -u ec2-user -- curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs > rustup.rs".to_string(),

        "chmod +x rustup.rs".to_string(),
        "chgrp ec2-user rustup.rs".to_string(),
        "chown ec2-user rustup.rs".to_string(),

        "sh ./rustup.rs -y".to_string(),
        "runuser -u ec2-user -- sh ./rustup.rs -y".to_string(),

        "./root/.cargo/bin/rustup update".to_string(),
        "runuser -u ec2-user -- ./.cargo/bin/rustup update".to_string(),
        // TODO sim link rustc from home/ec2-user/bin
        format!("ln -s /home/ec2-user/.cargo/bin/cargo {}/cargo", STATE.host_bin_path())


    ]).await.expect("Timed out")
}

async fn build_netbench_driver_cmd(
    host_group: &str,
    driver: &NetbenchDriver,
    ssm_client: &aws_sdk_ssm::Client,
    instance_ids: Vec<String>,
    unique_id: &str,
) -> SendCommandOutput {
    send_command(
        vec![Step::Configure],
        Step::BuildDriver(driver.driver_name.clone()),
        host_group,
        &format!("build_driver_{}", driver.proj_name),
        ssm_client,
        instance_ids,
        vec![
            // copy s3 to host
            // `aws s3 sync s3://netbenchrunnerlogs/2024-01-09T05:25:30Z-v2.0.1//SaltyLib-Rust/ /home/ec2-user/SaltyLib-Rust`
            format!(
                "aws s3 sync {}/{}/ {}/{}",
                STATE.s3_path(unique_id),
                driver.proj_name,
                STATE.host_home_path,
                driver.proj_name
            ),
        ]
        .into_iter()
        .chain(driver.ssm_build_cmd.clone().into_iter())
        .map(String::from)
        .collect(),
    )
    .await
    .expect("Timed out")
}

async fn build_russula_cmd(
    host_group: &str,
    ssm_client: &aws_sdk_ssm::Client,
    instance_ids: Vec<String>,
) -> SendCommandOutput {
    send_command(
        vec![Step::Configure],
        Step::BuildRussula,
        host_group,
        &format!("build_russula_{}", host_group),
        ssm_client,
        instance_ids,
        vec![
            format!(
                "git clone --branch {} {}",
                STATE.russula_branch, STATE.russula_repo
            )
            .as_str(),
            "cd netbench_orchestrator",
            format!("{}/cargo build", STATE.host_bin_path()).as_str(),
        ]
        .into_iter()
        .map(String::from)
        .collect(),
    )
    .await
    .expect("Timed out")
}
