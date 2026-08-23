# referee 子仓库可观测性优化建议

> 状态：提案（未实施）。本文档记录 Fluen 侧对 `crates/referee` 的后续优化需求，
> 供后续子仓库开发适配。当前 Fluen 已在 `src-tauri` 侧通过工具装饰器实现
> 子智能体可观测性（见 `src-tauri/src/agent_runtime/observability.rs` 与
> `src-tauri/src/motis_chat/agent_reporter.rs`），**不依赖**下列任何改动。

## 背景

Motis 通过 `delegate_agent` 把任务委派给子智能体（`referee-agent` 的
`AgentRuntime`），子智能体在内核内以**非流式** `engine.chat()` 运行完整的
「LLM ↔ 工具」循环。Fluen 侧目前能观测到的：委派起止（含耗时 / token /
失败原因）与子智能体**工具**调用的参数、结果、耗时；观测不到的：子智能体
**LLM 输出**（思考增量、文本增量）——它发生在 referee 引擎内部，无外部
回调可挂。

## 优化项（按价值排序）

### 1. 引擎事件钩子（Engine Event Hook）

**价值**：子智能体的 LLM 输出增量可观测（前端实时展示子智能体「正在写什么 /
在想什么」）；同时为 Usage/轮次统计提供统一出口。

**建议形态**（`referee-ai`）：

```rust
pub trait EngineObserver: Send + Sync {
    fn on_turn_started(&self, session_id: SessionId, turn_id: u64) {}
    fn on_thinking_delta(&self, session_id: SessionId, delta: &str) {}
    fn on_text_delta(&self, session_id: SessionId, delta: &str) {}
    fn on_turn_finished(&self, session_id: SessionId, turn_id: u64) {}
}

// EngineConfig / Engine::with_observer 注入；
// 非流式 run_turn 与流式 stream_loop 在消费 chunk 时同步回调。
```

Fluen 侧接入点：`agents.rs::build_agent_runtime` 构建时注入 observer，
由 `AgentReporter` 转为 `motis:agent-thought` / `motis:agent-text` 事件。

### 2. AgentRuntime 的流式转发桥

**价值**：`AgentRuntime::handle_chat` 目前固定走 `engine.chat()`（非流式）；
支持流式回信（RPC 回 envelope 流或分片消息）后，子智能体输出可真正
实时推送到前端，而不依赖轮次级回调。

**建议形态**：`SessionReply::Streaming { stream_id }` + 内核分片消息
（`StreamChunk { stream_id, delta }` / `StreamEnd { stream_id }`），
`Kernel::invoke` 返回首 envelope 后由调用方订阅后续分片。

### 3. `awaiting_calls_timeout` 落地

**现状**：`TimeoutConfig::awaiting_calls_timeout`（默认 60s）已定义，
但引擎回合循环未实际使用——工具等待完全由 executor 的 `tool_timeout` 治理。
Fluen 侧已把执行器超时调至 360s/660s，该配置形同虚设。

**建议**：要么在 `run_tool_calls`/`stream_loop` 的 AwaitingCalls 阶段接入
该 deadline（超时未完成的 pending 项进 DLQ、会话恢复 Idle），要么从
`TimeoutConfig` 移除，避免配置面误导。

### 4. ExecutorConfig 观测回调

**价值**：取代 Fluen 侧的 `ObservedTool` 装饰器（装饰器须重建注册表、
且观测不到 executor 层的超时/panic 折叠结果）。

**建议形态**（`referee-ai`）：

```rust
pub struct ExecutorConfig {
    // ... 既有字段
    pub on_execute: Option<Arc<dyn Fn(&ToolContext, &str, ToolExecEvent)>>,
}
pub enum ToolExecEvent { Started, Finished { ok: bool, duration_ms: u64 } }
```

executor 在 `execute_single` 的超时/panic 分支回调 `Finished`，装饰器
观测不到的错误路径（如 `ToolError::Timeout`）也能上报。

### 5. SessionReply 错误类型化

**现状**：`SessionReply::Error { message: String }` 把引擎的
`EngineReply::Error/Timeout` 折叠为字符串，`KernelError` 同样被
`delegate` 侧字符串化；上游无法程序化区分「超时 / 限流 / 引擎错误」。

**建议**：`SessionReply::Error { kind: ErrorKind, message: String }`，
`ErrorKind` 至少含 `Timeout / RateLimited / Llm / Internal`；`KernelError`
维持枚举并保证跨 envelope 保留。

### 6. 内核 invoke 心跳 / 进度

**价值**：长委派（Fluen 侧预算 10 分钟）期间，内核层可提供
「扩展仍存活」的心跳，供调用方区分「子智能体还在跑」与「RPC 通道假死」。

**建议形态**：`Kernel::invoke_with_progress(..., on_heartbeat)` 或
envelope 层心跳消息；扩展处理每完成一个阶段（一轮 LLM / 一批工具）时触发。

## 关联文档

- 超时分层：`src-tauri/src/motis_chat/timeouts.rs`（模块头注释含完整分层链）
- 现有观测实现：`src-tauri/src/agent_runtime/observability.rs`、
  `src-tauri/src/motis_chat/agent_reporter.rs`
- 前端展示：`src/views/main/components/motis/MotisAgentRun.vue`
