# Cursor BYOK 架构重构交接

## 使用方式

这是新对话的完整交接上下文。开始工作前：

1. 阅读根目录 `AGENTS.md`。
2. 阅读 `.agents/skills/cursor-prefix-stability/SKILL.md`。
3. 查看 `git status` 和当前 feature 分支最新提交。
4. 不要直接开始大规模移动 `server/src/cursor`；先完成本文“第一阶段测试护栏”。

## 本轮目标

本次不是功能缩减，而是代码模块消除和状态机收敛：

- 保留当前 Cursor Proto、工具、Checkpoint、恢复、打断、后台完成、子 Task、本地 BYOK 和官方上游转发能力。
- 以 `conversation_id` 为 Cursor 协议的核心状态身份。
- `request_id` 只作为可能重复的传输关联 ID，不能作为 Conversation、Run、Session 或 Trace 的唯一身份。
- 将 Cursor 生命周期收敛为一个状态写入者。
- 目录即架构，减少顶层模块数量，明确核心和扩展边界。
- 面向多人协作，禁止跨目录访问内部实现。

## 当前阶段已经完成的代码

本 feature 分支当前提交的是第一轮目录与模块整理，不包含后续 Conversation 状态机重写：

- Cursor Proto 唯一来源收敛到 `protocols/cursor/`。
- 根目录开发辅助工具统一移动到 `support/`：
  - `support/cursor-protocol-extractor/`
  - `support/cursor-capture/`
  - `support/benchmarks/`
- Desktop 前端按架构整理为：
  - `apps/desktop/src/features/`
  - `apps/desktop/src/shell/`
  - `apps/desktop/src/shared/`
- Server 的通用客户端端口并入 `server/src/run/`。
- `server/src/web/` 重命名为 `server/src/search/`。
- 模型观测数据结构聚合为 `server/src/model/observability.rs`。
- Release 辅助脚本移动到 `.github/scripts/`。
- 老配置导入能力保留，不应在后续重构中误删。

已完成过的验证：

- Workspace `cargo check` 通过。
- Desktop build 通过。
- `support/benchmarks/semble` crate check 通过。
- Cursor 生命周期相关测试曾通过：interrupt 12、error lifecycle 4、background completion 5、connect wire 6。

重新工作时仍需基于当前提交再次运行针对性验证，不能只依赖上述历史结果。

## 需要保留但未提交的独立工作区修改

`apps/docs/scripts/build-product-demo.mjs` 有一个独立的 Windows `spawnSync(..., shell: true)` 修正。它不属于本次架构提交，已从提交中排除；不要覆盖或删除。

## 当前实现规模

`server/src/cursor` 当前约 64 个 Rust 文件、约 1.9 万行。

主要分布：

| 模块 | 文件数 | 行数约 |
|---|---:|---:|
| `tools/` | 26 | 6382 |
| `request/` | 7 | 2785 |
| `checkpoint/` | 7 | 1159 |
| `interaction/` | 3 | 1105 |
| `session.rs` | 1 | 868 |
| `projection/` | 4 | 796 |

问题不是协议服务太复杂，而是同一条生命周期被多个模块分段持有。

## 当前协议链路

```text
Cursor BidiAppend
→ handlers.rs 选择 local/upstream
→ bidi_append.rs 解码 AgentClientMessage
→ CursorSessionRegistry.get_or_create(request_id)
→ CursorActor：append 排序、协议消息分发、Run 启动
→ CursorSession：Run/Tool/Checkpoint/Interrupt 协调
→ RunActor / RunEngine：通用模型循环和历史提交
→ OutputHub
→ RunSSE
```

主要客户端消息类型：

- `RunRequest`
- `ExecClientMessage`
- `ExecClientControlMessage`
- `KvClientMessage`
- `ConversationAction`
- `InteractionResponse`
- `ClientHeartbeat`

运行时 Action：

- `UserMessageAction`：软打断并继续当前 Conversation。
- `InjectContextAction`：软打断、追加运行时事件并继续。
- `CancelAction`：硬取消活动 Run。
- `CancelSubagentAction`：终止对应的普通子 Task 工具调用，不改变父 Conversation 生命周期。

主要服务端消息类型：

- `InteractionUpdate`
- `InteractionQuery`
- `ExecServerMessage`
- `ExecServerControlMessage`
- `ConversationCheckpointUpdate`
- Connect terminal frame

## 已确认的身份语义

| 身份 | 语义与约束 |
|---|---|
| `conversation_id` | 持久化 Conversation 的唯一核心身份 |
| `ParentConversationRef` | 子 Conversation 的不可变创建来源 |
| internal `run_id` | 服务端一次循环引擎执行，全局唯一 |
| Cursor `run_id` | `AgentRunRequest.run_id`，用于 `expected_run_id` 等 wire 关联 |
| `agent_session_id` | Cursor Agent 会话实例 |
| `request_id` | BidiAppend 与 RunSSE 的可重复传输关联 ID |
| `append_seqno` | 一个传输代次内的顺序，不是身份 |
| `message_id` / `injection_id` | 事件幂等身份 |
| `trace_id` | 一次链路观测的服务端唯一身份 |

身份约束：

- `request_id → conversation_id` 必须强关联。
- 同一个 `request_id` 可以在同一个 Conversation 内因重试或队列再次出现。
- 同一个 `request_id` 如果绑定到不同 Conversation，必须报协议冲突。
- 并发重复且 wire 无法区分时应拒绝冲突，不能猜测路由。
- 缺少 `conversation_id` 时，只能使用已经存在的 request binding；禁止回退到 `request_id`。
- `InjectContextAction.expected_run_id` 应比较 Cursor wire `run_id`，不能比较 `request_id`。

## ParentConversationRef 决策

任何根或子 Task 都是独立 Conversation。Conversation 不维护 children，不订阅子状态，不管理子生命周期。

父 Conversation 只看到一次普通 ToolCall。子 Conversation 只保存不可变的创建溯源：

```rust
struct ParentConversationRef {
    parent_conversation_id: ConversationId,
    parent_run_id: RunId,
    parent_tool_call_id: ToolCallId,
}
```

三个字段都必须保留：

- `parent_conversation_id`：属于哪条 Conversation 链。
- `parent_run_id`：父 Conversation 的哪次服务端 Run 创建。
- `parent_tool_call_id`：哪次普通 ToolCall 创建。

它不表示父子状态管理，也不应形成内存 children 树。

## 当前高风险点

### P0：request_id 被错误提升为核心身份

- `CursorSessionRegistry.runs` 使用 `HashMap<request_id, CursorSessionHandle>`。
- RunSSE 使用 request ID 查找和等待路由。
- Trace 表使用 `request_id TEXT PRIMARY KEY`。
- 父 Run 通过 `active_run_for_cursor_request(request_id)` 查找最近活动 Run。
- `request::prepare` 在缺失 Conversation ID 时回退到 request ID。

### P0：Wire Run 身份没有进入生命周期

- `AgentRunRequest.run_id`、`agent_session_id` 基本未使用。
- 内部 RunId 当前由 `request_id + UUID` 生成。
- `InjectContextAction.expected_run_id` 当前错误地与 `context.request_id` 比较。

### P0：生命周期有多个写入者

以下模块都能直接 cancel token、关闭 output 或发送 terminal：

- `bidi_append.rs`
- `actor.rs`
- `session.rs`
- `run_sse.rs`
- `sessions.rs`
- `lifecycle.rs`
- 通用 `RunEngine` 还会独立产生 `RunOutcome`

可能导致终态竞争、terminal 重复、错误原因不稳定。

### P0：Actor 与 Session 是重叠事件循环

- Actor 管 wire 输入、排序、工具回传和 runtime action。
- Session 管 Run 事件、工具状态、Checkpoint、interrupt 和 terminal。
- RunEngine 内还有第三层模型循环。

### P1：Transport 与 Run 生命周期绑定

- 当前 RunSSE Drop 会直接取消共享 Run token。
- 正确语义应为 `SSE disconnect = DetachTransport`，而不是必然 Cancel Run。

### P1：旁路状态

- `cancelled_conversations: HashSet<String>` 与 Store、RunOutcome、CancellationToken 平行存在。
- OutputHub 可以重放，但 Handle 关闭后 Registry 很快删除；晚到 RunSSE 可能永久等待。
- `CancelAction` 在 BidiAppend 和 Actor 两处处理。
- `CursorCommand::Abort` 没有生产发送者，主要只在测试中使用。
- `ClientCommand::Cancel` 没有 Cursor 生产发送者。

## 受保护的历史不变量

后续移动 `request / projection / checkpoint / run` 时，必须保持：

- 未发生 compaction 时，第 N 轮 provider history 是第 N+1 轮的严格结构前缀。
- 旧消息不能编辑、合并、重排或重新生成。
- request context 是 append-only 事件；内容相同不重复追加，变化时在本轮 runtime message 前追加。
- `A → B → A` 必须保留三个不同事件；同一事件重试必须幂等。
- Checkpoint encode/decode 必须保留 request-context wire identity。
- 自动 compaction 是显式前缀重置，只保留最新 request context。
- 后台完成和注入不能制造重复 request context。

## 目标状态模型

Conversation 本身没有 Completed/Cancelled 终态，结束的是 Run：

```text
Idle
→ Preparing
→ Modeling
↔ WaitingTools
→ Interrupting → Modeling
→ Checkpointing
→ Finalizing
→ Idle
```

唯一生命周期写入位置：

```text
server/src/cursor/conversation/runtime.rs
```

建议模型：

```rust
struct ConversationRuntime {
    identity: ConversationIdentity,
    active_run: Option<ActiveRun>,
    request_bindings: RequestBindings,
}

struct ConversationIdentity {
    conversation_id: ConversationId,
    parent: Option<ParentConversationRef>,
}

enum ConversationState {
    Idle,
    Active(ActiveRunState),
}

enum ActiveRunPhase {
    Preparing,
    Modeling,
    WaitingTools,
    Interrupting,
    Checkpointing,
    Finalizing,
}
```

所有其他模块只能向它发送命令或事件，不能直接关闭输出或修改生命周期。

## 目标目录

```text
protocols/
└── cursor/                         # Proto 唯一来源

server/src/cursor/
├── mod.rs                          # 只组合模块
├── protocol/                       # wire 类型与编解码
│   ├── mod.rs
│   ├── proto.rs
│   ├── wire.rs
│   └── identity.rs
├── gateway/                        # HTTP/Bidi/SSE 入口
│   ├── mod.rs
│   ├── bidi.rs
│   ├── stream.rs
│   └── routing.rs
├── conversation/                   # 唯一核心状态域
│   ├── mod.rs
│   ├── identity.rs
│   ├── registry.rs
│   ├── state.rs
│   ├── runtime.rs                  # 唯一生命周期写入者
│   ├── output.rs
│   ├── sync.rs
│   └── history/
│       ├── request/
│       ├── projection/
│       └── checkpoint/
├── tools/                          # 可扩展普通工具能力
│   ├── mod.rs                      # ToolPort
│   ├── codec.rs
│   ├── runtime.rs
│   ├── dispatch.rs
│   ├── completion.rs
│   ├── presentation.rs
│   ├── edit.rs
│   └── subtask.rs
├── prompting/                      # 无状态确定性编译
│   ├── mod.rs
│   ├── compiler.rs
│   └── assets.rs
├── apis/                           # 可替换产品能力
│   ├── mod.rs
│   ├── upstream.rs
│   ├── account.rs
│   ├── models.rs
│   ├── analytics.rs
│   └── tab.rs
└── observability.rs                # 只观察，不写生命周期
```

顶层架构概念最终收敛为：

```text
protocol
gateway
conversation
tools
prompting
apis
observability
```

## 核心与扩展边界

核心：

- Conversation identity 与 ParentConversationRef。
- RequestBinding 一致性。
- 单一生命周期状态机。
- canonical history、projection、checkpoint 与恢复。
- RunEngine 端口。
- terminal exactly-once。

允许扩展：

- 具体 Tool 实现，包括子 Task Tool。
- Cursor 官方上游代理。
- Account、Models、Analytics、Tab 等产品 API。
- Provider。
- Presentation 和 Observability sink。

Conversation 核心只认识 `ToolCall / ToolResult / ToolInterrupted`，不能认识或保存子 Conversation 状态。

## 模块消除映射

| 当前 | 目标 |
|---|---|
| `actor.rs + session.rs + command.rs + lifecycle.rs + inbox.rs` | `conversation/runtime.rs + state.rs` |
| `sessions.rs` | `conversation/registry.rs + output.rs` |
| `handlers.rs + bidi_append.rs + run_sse.rs` | `gateway/` |
| `connect.rs + proto.rs` | `protocol/` |
| `blob_sync.rs + context_sync.rs` | `conversation/sync.rs` |
| `request + projection + checkpoint` | `conversation/history/`，保留语义 |
| `interaction + json_stream + presentation + tools/stream` | `tools/presentation.rs + codec.rs` |
| `tools/dispatch/* + tools/result/*` | `dispatch.rs + completion.rs` |
| `proxy/account/model_catalog/analytics/tab` | `apis/` |

最终应删除：

- `cancelled_conversations` 旁路状态。
- `CursorCommand::Finished`。
- 无生产用途的 `CursorCommand::Abort`。
- 多处 `lifecycle::finish/cancel/fail`。
- 旧路径和兼容 re-export。

## 安全实施顺序

### 第一阶段：测试护栏，不移动目录

先增加行为测试：

- 同 request、同 Conversation、同事件：幂等。
- 同 request、同 Conversation、新 Cursor run：正确建立新代次。
- 同 request、不同 Conversation：协议冲突。
- `expected_run_id` 使用 wire run ID。
- ParentConversationRef 三字段准确并可持久恢复。
- RunSSE 先到、重连、晚到。
- cancel 发生在 prepare/model/tools/checkpoint 各阶段。
- soft interrupt 继续同一 Conversation。
- late ToolResult 不污染新一轮。
- final checkpoint 必须先于 terminal。
- terminal frame 恰好一次。
- SSE detach 与 CancelAction 独立。

### 第二阶段：身份收敛

- 引入强类型 ID。
- 消费 `AgentRunRequest.run_id` 和 `agent_session_id`。
- 建立 RequestBinding，不再假设 request ID 唯一。
- 删除 Conversation ID fallback。
- ParentConversationRef 改为三元组并持久化。
- Trace 使用独立 `trace_id`，不再以 request ID 为主键。
- 不修改工具、Checkpoint 或输出 wire 行为。

### 第三阶段：ConversationRegistry

- Registry 以 Conversation ID 为主键。
- request ID 仅作为反向路由索引。
- RunSSE 根据 binding 定位 Conversation。
- 不再通过 request ID 查询“最近父 Run”。

### 第四阶段：生命周期单写入者

- 所有 cancel 转为 ConversationCommand。
- SSE Drop 转为 DetachTransport。
- terminal 只能从 Conversation runtime 发出。
- 删除 `cancelled_conversations` 和重复 lifecycle 调用。

### 第五阶段：合并 Actor 与 Session

- 使用一个 `tokio::select!` 处理 wire、Run、Tool、Checkpoint。
- prepare 作为异步任务返回事件。
- 删除 `run_resources.take()` 和 `CursorCommand::Finished`。
- Run 结束后 Conversation 回到 Idle，可继续下一 Run。

### 第六阶段：目录和 Tools 收敛

- 生命周期稳定后再机械移动目录。
- 同一提交删除旧路径，不保留兼容层。
- 对 Tools 先锁定 wire 行为，再合并 codec、dispatch、completion、presentation。

## 多人协作规则

- `conversation/runtime.rs` 同一时间只由一个负责人修改。
- `mod.rs` 只组合和导出，不放业务。
- 跨目录只能使用公开 façade，禁止引用内部子模块。
- `tools` 不引用 `ConversationRuntime`。
- `apis` 不访问 Conversation 内部状态。
- `observability` 只能订阅事件。
- Provider 不认识 Cursor Proto。
- 一个 PR 不同时重写 identity、lifecycle 和 tool wire。
- 目录迁移设置短合并窗口，避免多人同时修改路径。
- 每阶段必须保持可编译、可测试、可回滚。

最终依赖方向：

```text
Gateway
  ↓
Conversation Runtime
  ├── History
  ├── ToolPort
  └── Generic Run
        ↓
      Provider

APIs → Gateway 公开接口
Observability ← 领域事件
```

## 新对话建议的第一条任务

```text
阅读 cursor.md、AGENTS.md 和 cursor-prefix-stability skill。不要立即移动目录。
先为 Cursor 身份建立 characterization tests，覆盖 request_id 重复、conversation_id 冲突、wire run_id 的 expected_run_id 校验，以及 ParentConversationRef 三元组。测试完成后给出最小身份收敛改动，不要同时重写生命周期和 Tools。
```
