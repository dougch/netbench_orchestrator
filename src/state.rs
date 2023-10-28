// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::ec2_utils::EndpointType;
use core::time::Duration;

pub const STATE: State = State {
    version: "v1.0.18",

    // git
    repo: "https://github.com/aws/s2n-quic.git",
    branch: "ak-netbench-sync",

    // aws
    s3_log_bucket: "netbenchrunnerlogs",
    // TODO contains request_response.json but that should just come from the orchestrator
    s3_resource_folder: "TS",
    cloudfront_url: "http://d2jusruq1ilhjs.cloudfront.net",
    cloud_watch_group: "netbench_runner_logs",
    // TODO remove `vpc_region` and configure vpc/subnet in same `region`
    region: "us-west-1",
    vpc_region: "us-east-1",
    instance_type: "c5.4xlarge",
    // Used to give permissions to the ec2 instance. Part of the IAM Role `NetbenchRunnerRole`
    instance_profile: "NetbenchRunnerInstanceProfile",
    // Used to find subnets with the following tag/value pair
    subnet_tag_value: (
        "tag:aws-cdk:subnet-name",
        "public-subnet-for-runners-in-us-east-1",
    ),
    // create/import a key pair to the account
    ssh_key_name: "apoorvko_m1",

    // orchestrator config
    host_count: HostCount {
        clients: 3,
        servers: 2,
    },
    workspace_dir: "./target/netbench",
    shutdown_time_sec: Duration::from_secs(60),
    russula_port: 8888,
};

pub struct State {
    pub version: &'static str,
    // git
    pub repo: &'static str,
    pub branch: &'static str,

    // aws
    pub s3_log_bucket: &'static str,
    pub s3_resource_folder: &'static str,
    pub cloudfront_url: &'static str,
    pub cloud_watch_group: &'static str,
    pub region: &'static str,
    // TODO we shouldnt need two different regions. create infra in the single region
    pub vpc_region: &'static str,
    pub instance_type: &'static str,
    pub instance_profile: &'static str,
    pub subnet_tag_value: (&'static str, &'static str),
    pub ssh_key_name: &'static str,

    // orchestrator config
    pub host_count: HostCount,
    pub workspace_dir: &'static str,
    pub shutdown_time_sec: Duration,
    pub russula_port: u16,
}

#[derive(Clone)]
pub struct HostCount {
    pub clients: u16,
    pub servers: u16,
}

impl State {
    pub fn cf_url(&self, unique_id: &str) -> String {
        format!("{}/{}", self.cloudfront_url, unique_id)
    }

    pub fn s3_path(&self, unique_id: &str) -> String {
        format!("s3://{}/{}", self.s3_log_bucket, unique_id)
    }

    // Create a security group with the following name prefix. Use with `sg_name_with_id`
    // security_group_name_prefix: "netbench_runner",
    pub fn security_group_name(&self, unique_id: &str) -> String {
        format!("netbench_{}", unique_id)
    }

    pub fn instance_name(&self, unique_id: &str, endpoint_type: EndpointType) -> String {
        format!("{}_{}", endpoint_type.as_str().to_lowercase(), unique_id)
    }
}
