# neatlogs

OpenTelemetry-native observability for LLM applications — TypeScript SDK.

Automatically trace LLM calls, agent workflows, tool invocations, and retrieval pipelines. Ship production-ready observability with a few lines of code.

## Quick Start

```typescript
import { init, span, shutdown, wrapOpenAI } from 'neatlogs';
import OpenAI from 'openai';

async function main() {
  // 1. Initialize the SDK
  await init({ apiKey: process.env.NEATLOGS_API_KEY });

  // 2. Explicitly wrap the provider client
  const client = wrapOpenAI(new OpenAI());

  // 3. Wrap functions with span() for observability
  const myWorkflow = span({ kind: 'WORKFLOW', name: 'qa-bot' }, async (query: string) => {
    const res = await client.chat.completions.create({
      model: 'gpt-4o',
      messages: [{ role: 'user', content: query }],
    });
    return res.choices[0].message.content;
  });

  const answer = await myWorkflow('What is TypeScript?');
  console.log(answer);

  await shutdown();
}

main().catch(console.error);
```

## Installation

```bash
npm install neatlogs
```

Install the provider or framework package you already use, then apply its
documented Neatlogs wrapper, hook, processor, or telemetry helper.

## Core Concepts

| Function | Purpose |
|----------|---------|
| `init()` | Initialize the SDK — sets up private OTel providers and exporters |
| `span()` | Wrap a function with observability — captures inputs, outputs, timing, and errors |
| `trace()` | Create a manual span with prompt template tracking and multi-turn session support |
| `log()` | Capture timestamped log steps within the active trace |
| `shutdown()` | Flush all pending data and shut down the SDK gracefully |

## Doctor v2

Run the local SDK pipeline check without credentials or network access:

```bash
npx neatlogs doctor --local --json
```

Run the controlled end-to-end probe explicitly:

```bash
NEATLOGS_API_KEY=<project-key> \
NEATLOGS_ENDPOINT=https://ingest.neatlogs.com \
npx neatlogs doctor --probe --json
```

Probe mode exports four generated spans through the normal `/v1/traces` route
with `x-neatlogs-doctor: v1`, flushes, and polls
`/api/traces/v3/:traceId` for that exact trace. It passes only after persisted
hierarchy, span semantics, input/output, versioned metadata, and numeric token
totals validate. It does not call an LLM or inspect user data.

### Important: Explicit integration

`init()` does not monkey-patch provider libraries. Use the documented explicit
wrapper, hook, processor, or telemetry helper for each integration.

```typescript
await init({ apiKey: process.env.NEATLOGS_API_KEY });
const client = wrapOpenAI(new OpenAI());
```

### Important: No Top-Level Await

Always wrap your code in an `async function main()` pattern:

```typescript
async function main() {
  await init({ ... });
  // ... your code
  await shutdown();
}

main().catch(console.error);
```

## API Reference

### `init(options?)`

Initialize the Neatlogs SDK. Returns `Promise<void>`.

```typescript
await init({
  apiKey: process.env.NEATLOGS_API_KEY,
  debug: true,
});
```

#### `InitOptions`

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `apiKey` | `string` | `process.env.NEATLOGS_API_KEY` | Neatlogs API key. Export disabled if not set. |
| `workflowName` | `string` | Derived from `process.argv[1]` | Name of the workflow being traced. |
| `userId` | `string` | — | User identifier for the session. |
| `tags` | `string[]` | — | Tags attached to all spans. |
| `metadata` | `Record<string, any>` | — | Custom metadata attached to all spans. |
| `debug` | `boolean` | `false` | Enable debug logging. |
| `disableExport` | `boolean` | `false` | Disable export to Neatlogs backend. |
| `tracerProvider` | `BasicTracerProvider` | Private SDK provider | Optional caller-owned private provider. It is never registered globally or shut down by Neatlogs. |
| `registerShutdownHandlers` | `boolean` | `true` for SDK-owned provider | Register process exit/signal handlers. Set `false` when the host application owns shutdown. |
| `mask` | `MaskFunction` | — | Global mask function applied to all spans. |
| `sampleRate` | `number` | `1.0` | Sampling rate (0.0 to 1.0). |
| `captureLogs` | `boolean` | `false` | Capture log records via OTel LoggerProvider. |
| `pii` | `'redact' &#124; 'hash' &#124; false` | — | PII detection mode. |
| `endpoint` | `string` | `'https://ingest.neatlogs.com'` | Base ingest endpoint. The SDK sends traces to `/v1/traces` and logs to `/v1/logs`. |
| `batchSize` | `number` | `100` | Maximum spans per export batch. |
| `flushInterval` | `number` | `5` | Seconds between batch flushes. |
| `piiEnabled` | `boolean` | — | Override team-level PII redaction toggle. |
| `piiSpanTypes` | `string[]` | — | Override which span types have server-side PII redaction. |
| `uploadAuthority` | `boolean \| UploadAuthority` | `false` | Enable the authenticated typed-media/oversized-OTLP upload contract, or inject an implementation. Keep disabled until the backend contract is deployed. |

---

### Independent `Client` pipelines

Use `Client` when one process must send different executions to different
Neatlogs projects. Each Client owns an isolated provider/export queue; the
active Client is scoped to its synchronous or asynchronous `activate()` call.

```typescript
import { Client, trace, wrapOpenAI } from 'neatlogs';

const project = new Client({
  apiKey: process.env.NEATLOGS_API_KEY!,
  workflowName: 'support-agent',
  captureLogs: true,
});
const openai = wrapOpenAI(rawOpenAI); // reusable; routing occurs at invocation

await project.activate(async () => {
  await trace({ name: 'answer', kind: 'WORKFLOW' }, async () => {
    return openai.responses.create({ model: 'gpt-5', input: 'Hello' });
  });
});

await project.shutdown();
```

Do not share one activation across unrelated concurrent jobs. Create one Client
per destination, use `activate()` around each execution, and always await
`shutdown()` when that Client is no longer needed.

---

### `span(options, fn)`

Wrap a function with OpenTelemetry span instrumentation. Returns a new function with the same signature that automatically creates a span when called.

```typescript
const myFn = span({ kind: 'WORKFLOW', name: 'my-workflow' }, async (input: string) => {
  return await process(input);
});

const result = await myFn('hello');
```

The `span()` function is a **higher-order function**: it takes your function and returns a new, instrumented version. The returned function has the same arguments and return type as the original.

#### `SpanOptions`

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `kind` | `SpanKind` | — | **Required.** The kind of span. |
| `name` | `string` | Function name | Custom name for the span. |
| `captureInput` | `boolean` | `true` | Capture function input. |
| `captureOutput` | `boolean` | `true` | Capture function output. |
| `captureStdout` | `boolean` | `false` | Capture stdout during execution. |
| `tags` | `string[]` | — | Tags for this span. |
| `metadata` | `Record<string, any>` | — | Custom metadata for this span. |
| `mask` | `MaskFunction` | — | Per-span mask function. |
| `internal` | `boolean` | — | Mark span as internal (not user-facing). |
| `role` | `string` | — | Agent role (for `kind: 'AGENT'`). |
| `goal` | `string` | — | Agent goal (for `kind: 'AGENT'`). |
| `toolName` | `string` | — | Tool name (for `kind: 'TOOL'`). |
| `parameters` | `Record<string, any>` | — | Tool parameters schema (for `kind: 'TOOL'`). |
| `model` | `string` | — | Embedding model name (for `kind: 'EMBEDDING'`). |
| `dimension` | `number` | — | Embedding dimension (for `kind: 'EMBEDDING'`). |

#### `SpanKind` Values

| Kind | Use For |
|------|---------|
| `WORKFLOW` | Top-level orchestration / pipelines |
| `AGENT` | Autonomous agents with roles and goals |
| `CHAIN` | Sequential processing steps |
| `TOOL` | External tool calls (APIs, databases, etc.) |
| `RETRIEVER` | Document / vector retrieval |
| `EMBEDDING` | Vector embedding operations |
| `MCP_TOOL` | Model Context Protocol tool calls |
| `GUARDRAIL` | Safety checks and content filters |

---

### `Span()` Decorator

TC39 Stage 3 class-method decorator for instrumenting class methods.

```typescript
class MyAgent {
  @Span({ kind: 'AGENT', role: 'researcher' })
  async run(query: string) {
    // automatically traced
    return await this.search(query);
  }

  @Span({ kind: 'TOOL', name: 'web-search' })
  async search(query: string) {
    return { results: ['...'] };
  }
}
```

> **Note:** Requires TypeScript 5.0+ with `"experimentalDecorators": false` (the new TC39 Stage 3 decorators, not legacy decorators).

---

### `trace(options, fn)`

Create a manual span that runs a callback. Unlike `span()`, which wraps a reusable function, `trace()` executes inline and is ideal for:

- **Prompt template tracking** — associate `PromptTemplate` instances with spans
- **Multi-turn sessions** — automatically creates root traces when `sessionId` is set
- **Grouping operations** — wrap a block of code in an ad-hoc span

```typescript
const result = await trace({
  name: 'llm-call',
  promptTemplate: myTemplate,
}, async (activeSpan) => {
  const rendered = myTemplate.compile({ name: 'world' });
  return await callLLM(rendered);
});
```

#### `TraceOptions`

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `name` | `string` | — | **Required.** Name for the trace span. |
| `kind` | `TraceSpanKind` | `'CHAIN'` | Span kind. Base kinds include `GUARDRAIL` and `EVALUATOR`; `trace()` additionally accepts `LLM`, `RERANKER`, and `VECTOR_STORE`. |
| `sessionId` | `string` | — | Session ID for grouping this root trace. |
| `parentSessionId` | `string` | — | Immediate parent session ID. |
| `sessionFeatureName` | `string` | — | Product feature that initiated the session request. |
| `sessionEntryPoint` | `string` | — | Application entry point that initiated the session request. |
| `promptTemplate` | `string &#124; PromptTemplate` | — | Prompt template to track. |
| `promptVariables` | `Record<string, any>` | — | Prompt variables for the template. |
| `userPromptTemplate` | `string &#124; UserPromptTemplate` | — | User prompt template. |
| `userPromptVariables` | `Record<string, any>` | — | User prompt variables. |
| `version` | `string` | — | Prompt version identifier. |
| `captureStdout` | `boolean` | `false` | Capture stdout during execution. |
| `mask` | `MaskFunction` | — | Per-trace mask function. |
| `attributes` | `Record<string, any>` | — | Custom attributes on the span. |
| `tags` | `string[]` | — | Tags for this trace. |
| `metadata` | `Record<string, any>` | — | Custom metadata. |

#### `span()` vs `trace()`

| | `span()` | `trace()` |
|---|----------|-----------|
| Pattern | Higher-order function wrapper | Inline callback |
| Reuse | Returns a reusable function | Executes immediately |
| Prompt tracking | No | Yes — `promptTemplate`, `promptVariables` |
| Session-aware | No | Yes — creates root traces for multi-turn sessions |
| Best for | Wrapping functions/methods | Ad-hoc tracing blocks, prompt versioning |

---

### `log(template, options?)`

Capture a timestamped log step within the current trace. Uses `{key}` placeholders for template variables.

```typescript
log('Processing query: {query}', { query: 'What is TypeScript?' });
log('Retrieved {count} documents in {ms}ms', { count: 5, ms: 120 });
log('Classification result', { category: 'technical', level: 'debug' });
```

Requires `captureLogs: true` in `init()`. Log records are emitted as OTel `LogRecord`s associated with the active span and exported to the OTLP logs endpoint at `/v1/logs`.

The special `level` key sets the log severity (`'info'`, `'debug'`, `'warn'`, `'error'`). All other keys are template variables and are also recorded as `log.{key}` attributes.

---

### `PromptTemplate` / `UserPromptTemplate`

Template classes for prompt versioning with `{{variable}}` placeholders. When used with `trace()`, variables are automatically captured on the span for prompt tracking.

```typescript
// String template
const systemPrompt = new PromptTemplate(
  'You are a {{role}} assistant specializing in {{topic}}.'
);

// Message array template
const chatPrompt = new PromptTemplate([
  { role: 'system', content: 'You are a {{role}} assistant.' },
  { role: 'user', content: '{{question}}' },
]);

// Compile with variables
const rendered = systemPrompt.compile({ role: 'helpful', topic: 'TypeScript' });
// => 'You are a helpful assistant specializing in TypeScript.'

// Access template metadata
systemPrompt.variables;  // ['role', 'topic']
systemPrompt.template;   // raw template string
```

`UserPromptTemplate` is identical but stores context separately — use it for the user/human turn in multi-template setups:

```typescript
const systemTpl = new PromptTemplate('You are a {{role}} assistant.');
const userTpl = new UserPromptTemplate('{{question}}');

await trace({
  name: 'qa',
  promptTemplate: systemTpl,
  userPromptTemplate: userTpl,
}, async () => {
  const system = systemTpl.compile({ role: 'helpful' });
  const user = userTpl.compile({ question: 'What is TypeScript?' });
  // Variables from both templates are captured on the span
});
```

---

### `PromptClient`

Server-side prompt management for storing, versioning, and retrieving prompts from the Neatlogs backend.

```typescript
import { PromptClient } from 'neatlogs';

const client = new PromptClient({
  baseUrl: 'https://ingest.neatlogs.com',
  apiKey: process.env.NEATLOGS_API_KEY!,
  cacheTtlMs: 60_000,              // fresh lifetime for latest/label lookups
  staleWhileRevalidateMs: 300_000, // bounded stale fallback during refresh
  requestTimeoutMs: 10_000,        // deadline for each prompt API request
  maxCacheEntries: 100,             // LRU bound for process memory
});

// Create a prompt
const prompt = await client.createPrompt({
  name: 'qa-system',
  content: 'You are a {{role}} assistant for {{company}}.',
  labels: ['production'],
});

// Fetch by name (returns latest version)
const handle = await client.getPrompt('qa-system');

// Per-key cache policy is retained across refreshes. During the stale window,
// getPrompt returns the last known value and starts one coalesced refresh.
const fastRefresh = await client.getPrompt('qa-system', {
  cacheTtlMs: 5_000,
  staleWhileRevalidateMs: 60_000,
});

// Fetch by label or version
const prod = await client.getPrompt('qa-system', { label: 'production' });
const v2 = await client.getPrompt('qa-system', { version: 2 });
const v3 = await client.getPrompt('qa-system', { version: 3 });

// Compile with variables
const rendered = handle.compile({ role: 'helpful', company: 'Acme' });

// Compile as message array
const messages = handle.compileMessages({ role: 'helpful', company: 'Acme' });

// List all prompts
const all = await client.listPrompts();

// Backward-compatible alias: managed prompts are immutable, so this creates a version
await client.updatePrompt('qa-system', { content: 'Updated: {{role}} for {{company}}.' });

// Save a new version. Content or messages is required by the backend contract.
await client.saveAsVersion('qa-system', {
  content: 'Version 2: {{role}} for {{company}}.',
  labels: ['staging'],
  commitMessage: 'Try the revised system prompt',
});

// Mutations target immutable version UUIDs. Name + version/label is resolved first.
await client.setLabel('qa-system', 'production', { version: 2 });
await client.addTag('qa-system', 'release-candidate', { version: 2 });
await client.removeTag('qa-system', 'release-candidate', { version: 2 });
await client.deletePrompt('qa-system', { version: 1 });

// Explicit PromptClient instances own their cache and prompt requests.
client.close();
```

Each prompt version may have zero or one active label. Accordingly, `labels`
accepts at most one value on create/save, and `setLabel()` replaces or moves
that label rather than adding a second simultaneous label.

Latest and label selectors are fresh for `cacheTtlMs`. After that, they may be
served only for the bounded `staleWhileRevalidateMs` window while one shared
same-key refresh runs. Refresh failure leaves the last known value available
until that stale window ends; after it ends, the next lookup waits for the
backend and reports a typed error. A version selector is immutable in-process:
`{ version: 2 }` never changes into another version. Request a different
version explicitly, call `clearCache()`, or create a new client.

Every request has a finite `requestTimeoutMs`. `close()` aborts in-flight prompt
requests and releases the cache; calls after close raise
`PromptClientClosedError`. The shared prompt client created by `init()` is
closed by `shutdown()`, without making prompt failures part of telemetry flush
success. An explicitly constructed `PromptClient` must be closed by its owner.

#### Prompt privacy and ownership

Prompt CRUD is intentional product-data transfer, separate from trace
telemetry. The API key selects the Neatlogs project and authenticates prompt
requests to `baseUrl`; the in-memory cache retains prompt content only until
eviction, `clearCache()`, or `close()`. Server retention follows the managed
prompt service policy for that project.

Telemetry `mask=`, `pii`, and `piiSpanTypes` settings do **not** transform prompt
content sent to the prompt-management API. If prompt content must be redacted,
transform it explicitly before calling prompt CRUD. The SDK does not currently
provide a prompt transform and does not claim that telemetry masking protects
managed prompt payloads.

Module-level convenience functions are also available after `init()`:

```typescript
import { init, getPrompt, fetchPrompt, listPrompts, createPrompt, updatePrompt, saveAsVersion, deletePrompt, setLabel, addTag, removeTag } from 'neatlogs';

await init({ apiKey: process.env.NEATLOGS_API_KEY });

const handle = await getPrompt('my-prompt');
const rendered = handle.compile({ name: 'world' });
```

---

### `flush()` / `flushAll()` / `flushAllDetailed()` / `shutdown()`

```typescript
// Flush pending spans without shutting down
await flush();

// Flush the default pipeline and every live Neatlogs Client under one deadline
const flushed = await flushAll(30_000);
if (!flushed) console.error('One or more Neatlogs pipelines failed to flush');

// Inspect per-pipeline timeout and failure details when needed
const result = await flushAllDetailed(30_000);
if (!result.success) console.error(result.outcomes);

// Flush and shut down — call before process exit
await shutdown();
```

`flushAll()` and `flushAllDetailed()` only know about Neatlogs-owned pipelines.
They do not discover or flush Datadog, Langfuse, Braintrust, or a global
OpenTelemetry provider.

`shutdown()` resets all SDK state so `init()` can be called again if needed.

---

### `bindTemplates(llm, systemTpl, userTpl?, compiledVars?)`

Bind prompt templates to a LangChain-compatible LLM so templates are automatically captured on LLM spans managed by frameworks like CrewAI.

```typescript
import { bindTemplates, PromptTemplate, UserPromptTemplate } from 'neatlogs';

const systemTpl = new PromptTemplate('You are a {{role}} assistant.');
const userTpl = new UserPromptTemplate('Research: {{topic}}');

const boundLlm = bindTemplates(llm, systemTpl, userTpl, { topic: 'AI safety' });
// Pass boundLlm to your framework — template context is injected on every invoke()
```

---

### `registerCrewaiTask(taskId, taskDescription)`

Register a CrewAI task for automatic span annotation.

```typescript
import { registerCrewaiTask } from 'neatlogs';

registerCrewaiTask('research-task', 'Research the latest AI developments');
```

## Supported TypeScript Integrations

For `neatlogs >=1.1.19 <2.0.0`, use only the explicit helper shown below. The
SDK has no `instrumentations: [...]` loader. These helpers attach to the object,
callback surface, processor, or plugin you pass and use Neatlogs' private
context. Versioned rows below name the dependencies installed by this repository's test
matrix; API-shaped rows deliberately make no blanket semver claim.

| Library | Repository test/API baseline | Explicit helper | Import path |
|---|---|---|---|
| OpenAI | `openai` 6.34.x | `wrapOpenAI(client)` | `neatlogs` or `neatlogs/openai` |
| Anthropic | `@anthropic-ai/sdk` 0.68.x | `wrapAnthropic(client)` | `neatlogs` or `neatlogs/anthropic` |
| Azure OpenAI | `openai` 6.34.x | `wrapAzureOpenAI(client)` | `neatlogs/azure-openai` |
| AWS Bedrock Runtime | AWS SDK v3 command API | `wrapBedrock(client)` | `neatlogs/bedrock` |
| Google GenAI | `@google/genai` 1.34.x | `wrapGoogleGenAI(client)` / `wrapGoogleGenAIChat(chat)` | `neatlogs/google-genai` |
| Vertex AI through `@google/genai` | `@google/genai` 1.34.x | `wrapVertexAI(client)` / `wrapVertexAIChat(chat)` | `neatlogs/vertex-ai` |
| OpenRouter Agent | `@openrouter/agent` 0.7.x | `wrapOpenRouterAgent(client)` / `wrapCallModel(fn)` | `neatlogs/openrouter-agent` |
| Vercel AI SDK | `ai` 6.x and 7.x | `wrapAISDK(ai)` | `neatlogs/ai` |
| Mastra | `@mastra/core` 1.32.x | `wrapMastra(entity)` / `wrapMastraRerank(fn)` | `neatlogs/mastra` |
| Claude Agent SDK | documented `query()` API | `wrapClaudeAgentSDK(sdk)` | `neatlogs/claude-agent-sdk` |
| LangChain / LangGraph | `@langchain/core` 0.3.x | `langchainHandler()` callback | `neatlogs` or `neatlogs/langchain` |
| OpenAI Agents SDK | documented `addTraceProcessor()` API | `openaiAgentsProcessor()` | `neatlogs` or `neatlogs/openai-agents` |
| Pi Agent | `agent-core` 0.73.x and 0.83.x | `piAgentHooks(agent)` / `tracePiAgentEvents(...)` / `tracePiStream(...)` | `neatlogs` or `neatlogs/pi-agent` |
| OpenCode | current plugin API | `NeatlogsOpencodePlugin` | `neatlogs/opencode` |
| Browser client | browser SDK API in this release | `Neatlogs` | `neatlogs/browser` |

Edge runtime packaging, the removed `instrumentations` init option, and Strands
global-context hooks are not supported. `strandsHooks()` remains an explicit
runtime rejection so an application cannot silently believe it is isolated or
instrumented.

```typescript
// Vercel AI SDK
import { init, shutdown } from 'neatlogs';
import { wrapAISDK } from 'neatlogs/ai';
import * as ai from 'ai';
import { openai } from '@ai-sdk/openai';

await init({ apiKey: process.env.NEATLOGS_API_KEY });
const { generateText, ToolLoopAgent } = wrapAISDK(ai);

const { text } = await generateText({
  model: openai('gpt-4o-mini'),
  prompt: 'What is TypeScript?',
});

// Agents are supported in both versions. The wrapper selects `telemetry` for
// AI SDK v7 and `experimental_telemetry` for AI SDK v6 automatically.
const agent = new ToolLoopAgent({
  id: 'support-agent',
  model: openai('gpt-4o-mini'),
  telemetry: { functionId: 'support-agent' }, // use experimental_telemetry on v6
});
await agent.generate({ prompt: 'Help me debug my order' });

await shutdown();
```

AI SDK v7 users must also install the telemetry adapter with
`npm install @ai-sdk/otel`. AI SDK v6 users do not need that package.

If a call supplies a tracer through `createAITelemetry` (for example, Laminar),
the wrapper mirrors the AI SDK's native spans to both that tracer and Neatlogs.
The caller-owned tracer remains the global context owner; Neatlogs keeps
separate parent context and export state in its private provider.

The same coexistence is available without the wrapper by passing the existing
tracer to `createAITelemetry`:

```typescript
import { getTracer } from '@lmnr-ai/lmnr';
import { streamText } from 'ai';
import { createAITelemetry } from 'neatlogs/ai';

await streamText({
  model,
  prompt,
  telemetry: createAITelemetry({ // use experimental_telemetry on AI SDK v6
    tracer: getTracer(),
    functionId: 'progress-narration',
  }),
});
```

## Configuration

### Environment Variables

| Variable | Description |
|----------|-------------|
| `NEATLOGS_API_KEY` | API key (fallback when `apiKey` option is not provided) |
| `NEATLOGS_DISABLE_EXPORT` | Set to `true`, `1`, or `yes` to disable export |
| `NEATLOGS_UPLOADS_ENABLED` | Set to `true`, `1`, or `yes` to enable authenticated typed-media and oversized-OTLP uploads |

### Programmatic Configuration

All configuration is passed via `init()` options. See the [InitOptions table](#initoptions) above.

```typescript
await init({
  apiKey: process.env.NEATLOGS_API_KEY,
  workflowName: 'my-pipeline',
  userId: 'user-456',
  tags: ['production', 'v2'],
  metadata: { environment: 'prod' },
  sampleRate: 0.5,
  captureLogs: true,
  debug: true,
});
```

## PII Masking

### Global Mask

Apply a mask function to all spans:

```typescript
await init({
  apiKey: process.env.NEATLOGS_API_KEY,
  mask: (spanData) => {
    // Redact email addresses
    for (const [key, value] of Object.entries(spanData)) {
      if (typeof value === 'string') {
        spanData[key] = value.replace(/[\w.-]+@[\w.-]+/g, '[REDACTED]');
      }
    }
    return spanData;
  },
});
```

### Per-Span Mask

Apply a mask to a specific span:

```typescript
const sensitive = span({
  kind: 'TOOL',
  name: 'user-lookup',
  mask: (spanData) => {
    delete spanData['input.value'];
    return spanData;
  },
}, async (userId: string) => {
  return await lookupUser(userId);
});
```

### Per-Trace Mask

```typescript
await trace({
  name: 'sensitive-operation',
  mask: (spanData) => {
    // Return null to drop the span entirely
    return null;
  },
}, async () => {
  // This span will not be exported
});
```

### Server-Side PII Redaction

```typescript
await init({
  apiKey: process.env.NEATLOGS_API_KEY,
  pii: 'redact',          // or 'hash' or false
  piiEnabled: true,        // override team-level toggle
  piiSpanTypes: ['LLM'],   // only redact LLM spans
});
```

## Examples

See the [`examples/`](./examples/) directory for complete, runnable examples:

| File | Description |
|------|-------------|
| [`basic-openai.ts`](./examples/basic-openai.ts) | Basic OpenAI usage with an explicit wrapper |
| [`prompt-management.ts`](./examples/prompt-management.ts) | PromptTemplate + trace() for prompt versioning |
| [`multi-agent-workflow.ts`](./examples/multi-agent-workflow.ts) | Nested spans: WORKFLOW → AGENT → TOOL |
| [`custom-spans.ts`](./examples/custom-spans.ts) | All span kinds: WORKFLOW, CHAIN, AGENT, TOOL, RETRIEVER, EMBEDDING, GUARDRAIL |
| [`sdk_examples/ai_sdk_basic/`](./examples/sdk_examples/ai_sdk_basic/) | Vercel AI SDK via `wrapAISDK` — generateText + streamText + tools |

Run any example with:

```bash
NEATLOGS_API_KEY=your-key npx tsx examples/basic-openai.ts
```

## License

MIT
