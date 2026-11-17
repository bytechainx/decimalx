#!/usr/bin/env node
// 生成 decimalx-prod-live E1 机器收据（trust_level=E1）。
// 语义对齐 domainx-prod-live-receipt.mjs：收据绑定 CI_COMMIT_SHA，
// 记录 pipeline / job / 消费者 / profile，由 CI job 断言字段后归档为 artifact。
// 环境变量缺失或非常规运行属工具错误，退出码 2；内容不一致属内容发现，退出码 1。
import { writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";

const RECEIPT_PATH = process.env.RECEIPT_OUT || "decimalx-live-receipt.json";
const CONSUMER_MANIFEST = "Cargo.toml";
const CONSUMER_EXAMPLE = "decimalx_basic";
const CONSUMER_MARKER = "decimalx-consumer: ok";

function toolError(message) {
  console.error(`prod-live-receipt 工具错误：${message}`);
  process.exit(2);
}

function git(args) {
  const result = spawnSync("git", args, { encoding: "utf8" });
  if (result.error) toolError(`git ${args.join(" ")}: ${result.error.message}`);
  if (result.status !== 0) toolError(`git ${args.join(" ")}: ${(result.stderr || "").trim()}`);
  return result.stdout.trim();
}

const commitSha = process.env.CI_COMMIT_SHA;
if (!commitSha || !/^[0-9a-f]{40}$/.test(commitSha)) {
  toolError("CI_COMMIT_SHA 缺失或不是 40 位十六进制 SHA");
}
if (git(["rev-parse", "HEAD"]) !== commitSha) {
  console.error(`prod-live-receipt 内容发现：HEAD 与 CI_COMMIT_SHA 不一致`);
  process.exit(1);
}

const profile = process.env.DECIMALX_LIVE_PROFILE;
if (!profile) toolError("DECIMALX_LIVE_PROFILE 未设置");

const headTree = git(["rev-parse", "HEAD^{tree}"]);
const receipt = {
  schema: "decimalx-prod-live-receipt/v1",
  trust_level: "E1",
  evidence_id: `E1-decimalx-live-${process.env.CI_JOB_NAME || "local"}-${process.env.CI_PIPELINE_ID || "local"}`,
  commit_sha: commitSha,
  tree_sha: headTree,
  pipeline_id: process.env.CI_PIPELINE_ID || null,
  job_id: process.env.CI_JOB_ID || null,
  job_name: process.env.CI_JOB_NAME || null,
  consumer: {
    kind: "example",
    manifest: CONSUMER_MANIFEST,
    example: CONSUMER_EXAMPLE,
    expected_marker: CONSUMER_MARKER,
    profile,
  },
  generated_at: new Date().toISOString(),
};

writeFileSync(RECEIPT_PATH, `${JSON.stringify(receipt, null, 2)}\n`);
console.log(`prod-live-receipt: wrote ${RECEIPT_PATH} (commit ${commitSha.slice(0, 9)})`);
