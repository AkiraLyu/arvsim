# `scripts/`：构建、测试与交互脚本审查记录

> 本文属于代码审查报告（基线：提交 `17ad107` 的当前工作区，2026-08-12），只记录本轮仍成立的审查发现与修改建议，未改动实现代码。总览见 [README](./README.md)。对应的现状文档：[scripts.md](/home/akira/codespace/arvsim/docs/repository-status/scripts.md)。

## 审查范围与总体判断

覆盖 `build_xv6_fixture.sh`、`run_testbench.sh` 和 `run_xv6_cli.sh` 的参数、环境覆盖、退出行为、fixture 来源、生成 runner 和长期交互资源。上一轮的 edition、rlib 选择、路径和原始字节问题已修复；当前仍有两条中危和一条低危问题。

共 3 条发现：高危 0、中危 2、低危 1。

## 发现的问题

### 1. `fetch` 失败会复用任意旧 checkout，并把请求的仓库和 ref 写入元数据

- 位置：[`scripts/build_xv6_fixture.sh:39`](/home/akira/codespace/arvsim/scripts/build_xv6_fixture.sh#L39)、[`scripts/build_xv6_fixture.sh:45`](/home/akira/codespace/arvsim/scripts/build_xv6_fixture.sh#L45)、[`scripts/build_xv6_fixture.sh:56`](/home/akira/codespace/arvsim/scripts/build_xv6_fixture.sh#L56)、[`scripts/build_xv6_fixture.sh:63`](/home/akira/codespace/arvsim/scripts/build_xv6_fixture.sh#L63)
- 分级：中危 · 可复现性
- 备注：状态文档已提到“更新失败后复用旧版本”，本条补充元数据失实和显式覆盖失效的后果

已有目录下，脚本先把 `origin` 改为当前 `XV6_REPO`；如果获取 `XV6_REF` 失败，只打印警告并继续构建当前 `HEAD`。该 `HEAD` 可能来自旧仓库、旧分支或完全不同的显式 ref。随后生成的 `fixture.env` 却把请求的 `XV6_REPO` 和 `XV6_REF` 与实际旧提交并列记录，使读取者误以为覆盖已经生效。

这会把网络、拼写或权限错误悄悄变成陈旧但可能通过的测试结果；显式要求某个安全修复或回归提交时尤其危险。

**修改建议：**

```bash
git -C "$DEST" remote set-url origin "$XV6_REPO"
if ! git -C "$DEST" fetch --depth 1 origin "$XV6_REF"; then
  printf 'error: could not fetch xv6 ref %s from %s\n' "$XV6_REF" "$XV6_REPO" >&2
  exit 1
fi
git -C "$DEST" checkout --detach FETCH_HEAD
```

若确实需要离线复用，增加默认关闭的显式选项（如 `ARVSIM_XV6_ALLOW_STALE=1`），验证当前 commit、origin 和已记录元数据一致，并在 `fixture.env` 中分别记录请求来源和实际 commit，不得把复用路径描述成已更新成功。

### 2. `ARVSIM_XV6_CLI_STEP_CHUNK=0` 让 runner 永久空转且 boot timeout 永不触发

- 位置：[`scripts/run_xv6_cli.sh:80`](/home/akira/codespace/arvsim/scripts/run_xv6_cli.sh#L80)、[`scripts/run_xv6_cli.sh:89`](/home/akira/codespace/arvsim/scripts/run_xv6_cli.sh#L89)、[`scripts/run_xv6_cli.sh:128`](/home/akira/codespace/arvsim/scripts/run_xv6_cli.sh#L128)、[`scripts/run_xv6_cli.sh:151`](/home/akira/codespace/arvsim/scripts/run_xv6_cli.sh#L151)
- 分级：中危 · 健壮性

环境值 `0` 能成功解析为 `usize`，因此不会回退默认值。`0..step_chunk` 循环不执行，`steps` 永远保持 0，等待 shell 的预算判断也永远为假；`--boot-only` 和交互模式都会在宿主上无限忙轮询。非法字符串则被静默替换成默认值，与 xv6 测试预算的严格解析行为不一致。

**修改建议：**

```rust
fn env_positive_usize(name: &str, default: usize) -> Result<usize, Box<dyn Error>> {
    let Some(value) = env::var_os(name) else {
        return Ok(default);
    };
    let text = value.to_string_lossy();
    let parsed: usize = text
        .parse()
        .map_err(|error| format!("{name}={text:?} is invalid: {error}"))?;
    if parsed == 0 {
        return Err(format!("{name} must be greater than zero").into());
    }
    Ok(parsed)
}
```

对 step chunk 强制大于零；对 boot steps 明确决定 0 是“立即超时”还是非法值，并测试该合同。步数累加还应使用 `checked_add`，避免极长会话回绕后重新绕过预算。

### 3. 交互 runner 永不丢弃已打印的 UART 输出，长会话会持续占用宿主内存

- 位置：[`scripts/run_xv6_cli.sh:114`](/home/akira/codespace/arvsim/scripts/run_xv6_cli.sh#L114)、[`scripts/run_xv6_cli.sh:135`](/home/akira/codespace/arvsim/scripts/run_xv6_cli.sh#L135)、[`src/uart.rs:71`](/home/akira/codespace/arvsim/src/uart.rs#L71)、[`src/uart.rs:112`](/home/akira/codespace/arvsim/src/uart.rs#L112)
- 分级：低危 · 健壮性

`BufferedUartBackend` 把全部输出追加到 `Vec<u8>`；runner 只推进 `printed` 索引，从未删除已转发字节。长期交互、持续日志或 guest 输出洪泛会让宿主内存按累计输出量线性增长。当前 `clear_output()` 不能直接在 runner 中使用，因为清空会让索引失效，也可能丢掉尚未写入 stdout 的内容。

**修改建议：**

为缓冲后端增加所有权明确的增量提取接口，或为交互程序实现直接流式后端：

```rust
pub fn take_output(&self) -> Vec<u8> {
    std::mem::take(&mut self.0.borrow_mut().output)
}
```

runner 每轮写完 `take_output()` 的返回值后即可释放历史内容。shell 提示符匹配只需保留最后一个字节与新块拼接，不需要完整历史；测试辅助仍可选择保留完整缓冲用于失败诊断。
