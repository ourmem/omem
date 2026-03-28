# omem API Reference

面向前端开发者的完整 REST API 参考文档。

## 概览

| 项目 | 说明 |
|------|------|
| Base URL | `http://localhost:8080` |
| 协议 | HTTP/HTTPS |
| 数据格式 | JSON（除文件上传外） |
| 字符编码 | UTF-8 |

### 认证方式

需要认证的端点通过 `X-API-Key` 请求头传递 API 密钥。密钥在创建租户时返回。

```
X-API-Key: 550e8400-e29b-41d4-a716-446655440000
```

可选的 `X-Agent-Id` 请求头用于多 Agent 场景下的隔离：

```
X-Agent-Id: coder
```

**公开端点**（不需要认证）：
- `GET /health`
- `POST /v1/tenants`
- `POST /v1/connectors/github/webhook`

其余所有 `/v1/*` 端点都需要 `X-API-Key`。

### 通用错误格式

所有错误返回统一的 JSON 结构：

```json
{
  "error": {
    "code": "not_found",
    "message": "not found: memory 550e8400-e29b-41d4-a716-446655440000"
  }
}
```

| HTTP 状态码 | error.code | 触发条件 |
|-------------|------------|----------|
| 400 | `validation_error` | 请求参数缺失或格式错误 |
| 401 | `unauthorized` | 缺少 API Key、Key 无效、租户已停用 |
| 404 | `not_found` | 资源不存在 |
| 429 | `rate_limited` | 请求频率超限 |
| 500 | `internal_error` | 存储/嵌入/LLM/内部错误 |

### 数据类型说明

| 类型 | 格式 | 示例 |
|------|------|------|
| UUID | v4 字符串 | `"550e8400-e29b-41d4-a716-446655440000"` |
| 时间戳 | ISO 8601 / RFC 3339 | `"2025-01-15T10:30:00+00:00"` |
| 浮点数 | 32 位 | `0.85` |
| 布尔值 | JSON boolean | `true` / `false` |

---

## 一、租户管理

### POST /v1/tenants

创建新租户。返回的 `id` 同时作为 API Key 使用。创建租户时会自动创建一个 personal 类型的 Space。

**认证**: 不需要

**Request Body**:

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| name | string | 是 | 租户名称 |

```json
{
  "name": "my-workspace"
}
```

**Response** `200 OK`:

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "api_key": "550e8400-e29b-41d4-a716-446655440000",
  "status": "active"
}
```

> `id` 和 `api_key` 是同一个 UUID。后续所有请求用这个值作为 `X-API-Key`。

**错误**:

| 状态码 | 条件 |
|--------|------|
| 400 | `name` 为空 |

**curl 示例**:

```bash
curl -X POST http://localhost:8080/v1/tenants \
  -H "Content-Type: application/json" \
  -d '{"name": "my-workspace"}'
```

---

## 二、记忆 CRUD

### POST /v1/memories

创建记忆。支持两种模式：消息摄入（异步）和直接创建（同步）。

**认证**: 需要 `X-API-Key`

#### 模式一：消息摄入（Smart Ingest）

传入对话消息，由 LLM 提取记忆。返回 `202 Accepted`。

**Request Body**:

| 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| messages | array | 是 | - | 消息数组 |
| messages[].role | string | 是 | - | 角色：`"user"` 或 `"assistant"` |
| messages[].content | string | 是 | - | 消息内容 |
| mode | string | 否 | `"smart"` | `"smart"` 智能提取，`"raw"` 原样存储 |
| agent_id | string | 否 | null | Agent 标识（也可通过 `X-Agent-Id` header 传递） |
| session_id | string | 否 | null | 会话标识 |
| entity_context | string | 否 | null | 额外的实体上下文信息 |

```json
{
  "messages": [
    {"role": "user", "content": "I switched from VS Code to Zed last week"},
    {"role": "assistant", "content": "Nice! Zed has great Rust support."}
  ],
  "mode": "smart",
  "agent_id": "coder",
  "session_id": "session-001"
}
```

**Response** `202 Accepted`:

```json
{
  "task_id": "550e8400-e29b-41d4-a716-446655440000",
  "stored_count": 2
}
```

#### 模式二：直接创建

传入 `content` 字段，直接创建一条 pinned 类型的记忆。返回 `201 Created`。

**Request Body**:

| 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| content | string | 是 | - | 记忆内容 |
| tags | string[] | 否 | `[]` | 标签列表 |
| source | string | 否 | null | 来源标识 |

```json
{
  "content": "User prefers dark mode in all editors",
  "tags": ["preferences", "editor"],
  "source": "manual"
}
```

**Response** `201 Created`:

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "content": "User prefers dark mode in all editors",
  "l0_abstract": "",
  "l1_overview": "",
  "l2_content": "User prefers dark mode in all editors",
  "category": "preferences",
  "memory_type": "pinned",
  "state": "active",
  "tier": "peripheral",
  "importance": 0.5,
  "confidence": 0.5,
  "access_count": 0,
  "tags": ["preferences", "editor"],
  "scope": "global",
  "agent_id": null,
  "session_id": null,
  "tenant_id": "550e8400-e29b-41d4-a716-446655440000",
  "source": "manual",
  "relations": [],
  "superseded_by": null,
  "invalidated_at": null,
  "created_at": "2025-01-15T10:30:00+00:00",
  "updated_at": "2025-01-15T10:30:00+00:00",
  "last_accessed_at": null,
  "space_id": "",
  "visibility": "global",
  "owner_agent_id": "",
  "provenance": null
}
```

**错误**:

| 状态码 | 条件 |
|--------|------|
| 400 | `messages` 和 `content` 都没有提供 |
| 400 | `messages` 数组为空 |
| 400 | `content` 为空字符串 |

**curl 示例**:

```bash
# 模式一：消息摄入
curl -X POST http://localhost:8080/v1/memories \
  -H "Content-Type: application/json" \
  -H "X-API-Key: YOUR_API_KEY" \
  -d '{
    "messages": [
      {"role": "user", "content": "I prefer using Rust for backend services"},
      {"role": "assistant", "content": "Rust is great for performance-critical backends!"}
    ],
    "mode": "smart"
  }'

# 模式二：直接创建
curl -X POST http://localhost:8080/v1/memories \
  -H "Content-Type: application/json" \
  -H "X-API-Key: YOUR_API_KEY" \
  -d '{"content": "User prefers dark mode", "tags": ["preferences"]}'
```

---

### GET /v1/memories/search

语义搜索记忆。支持跨 Space 搜索。

**认证**: 需要 `X-API-Key`

**Query 参数**:

| 参数 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| q | string | 是 | - | 搜索查询文本 |
| limit | integer | 否 | 20 | 最大返回数量 |
| scope | string | 否 | null | 按 scope 过滤 |
| min_score | float | 否 | null | 最低相关性分数 |
| include_trace | boolean | 否 | false | 是否返回检索管线追踪信息 |
| space | string | 否 | null | 搜索的 Space 范围。`"all"` 搜索所有，逗号分隔指定多个 Space ID |

**Response** `200 OK`:

```json
{
  "results": [
    {
      "memory": {
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "content": "User prefers dark mode in all editors",
        "l0_abstract": "User prefers dark mode",
        "l1_overview": "The user has a strong preference for dark mode across all code editors and IDEs.",
        "l2_content": "User prefers dark mode in all editors",
        "category": "preferences",
        "memory_type": "insight",
        "state": "active",
        "tier": "core",
        "importance": 0.85,
        "confidence": 0.92,
        "access_count": 12,
        "tags": ["preferences", "editor", "theme"],
        "scope": "global",
        "agent_id": "coder",
        "session_id": "session-001",
        "tenant_id": "550e8400-e29b-41d4-a716-446655440000",
        "source": null,
        "relations": [],
        "superseded_by": null,
        "invalidated_at": null,
        "created_at": "2025-01-15T10:30:00+00:00",
        "updated_at": "2025-01-16T08:00:00+00:00",
        "last_accessed_at": "2025-01-20T14:22:00+00:00",
        "space_id": "personal/550e8400-e29b-41d4-a716-446655440000",
        "visibility": "global",
        "owner_agent_id": "coder",
        "provenance": null
      },
      "score": 0.87
    }
  ],
  "trace": null
}
```

**带 trace 的响应**（`include_trace=true`）：

```json
{
  "results": [],
  "trace": {
    "stages": [
      {
        "name": "parallel_search",
        "input_count": 0,
        "output_count": 42,
        "duration_ms": 12.5,
        "score_range": [0.31, 0.92]
      },
      {
        "name": "rrf_fusion",
        "input_count": 42,
        "output_count": 35,
        "duration_ms": 0.8,
        "score_range": [0.33, 0.89]
      }
    ],
    "total_duration_ms": 45.2,
    "final_count": 10
  }
}
```

**错误**:

| 状态码 | 条件 |
|--------|------|
| 400 | `q` 参数为空 |

**curl 示例**:

```bash
# 基本搜索
curl "http://localhost:8080/v1/memories/search?q=editor+preferences&limit=5" \
  -H "X-API-Key: YOUR_API_KEY"

# 带 trace
curl "http://localhost:8080/v1/memories/search?q=editor+preferences&include_trace=true" \
  -H "X-API-Key: YOUR_API_KEY"

# 指定 Space 搜索
curl "http://localhost:8080/v1/memories/search?q=architecture&space=team/backend" \
  -H "X-API-Key: YOUR_API_KEY"

# 搜索所有 Space
curl "http://localhost:8080/v1/memories/search?q=architecture&space=all" \
  -H "X-API-Key: YOUR_API_KEY"
```

---

### GET /v1/memories/{id}

获取单条记忆详情。

**认证**: 需要 `X-API-Key`

**路径参数**:

| 参数 | 类型 | 说明 |
|------|------|------|
| id | string | 记忆 UUID |

**Response** `200 OK`:

返回完整的 Memory 对象（结构同上）。

**错误**:

| 状态码 | 条件 |
|--------|------|
| 404 | 记忆不存在 |

**curl 示例**:

```bash
curl http://localhost:8080/v1/memories/550e8400-e29b-41d4-a716-446655440000 \
  -H "X-API-Key: YOUR_API_KEY"
```

---

### PUT /v1/memories/{id}

更新记忆的内容、标签或状态。更新 `content` 会触发重新嵌入。

**认证**: 需要 `X-API-Key`

**路径参数**:

| 参数 | 类型 | 说明 |
|------|------|------|
| id | string | 记忆 UUID |

**Request Body**:

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| content | string | 否 | 新内容（不能为空字符串，会触发重新嵌入） |
| tags | string[] | 否 | 替换标签列表 |
| state | string | 否 | 新状态：`"active"`, `"archived"`, `"deleted"` |

```json
{
  "content": "User prefers Catppuccin Mocha dark theme",
  "tags": ["preferences", "editor", "theme"],
  "state": "active"
}
```

**Response** `200 OK`:

返回更新后的完整 Memory 对象。

**错误**:

| 状态码 | 条件 |
|--------|------|
| 400 | `content` 为空字符串 |
| 400 | `state` 值无效 |
| 404 | 记忆不存在 |

**curl 示例**:

```bash
curl -X PUT http://localhost:8080/v1/memories/550e8400-e29b-41d4-a716-446655440000 \
  -H "Content-Type: application/json" \
  -H "X-API-Key: YOUR_API_KEY" \
  -d '{
    "content": "User prefers Catppuccin Mocha dark theme",
    "tags": ["preferences", "editor", "theme"]
  }'
```

---

### DELETE /v1/memories/{id}

软删除记忆（将 state 设为 `"deleted"`，数据不会物理删除）。

**认证**: 需要 `X-API-Key`

**路径参数**:

| 参数 | 类型 | 说明 |
|------|------|------|
| id | string | 记忆 UUID |

**Response** `200 OK`:

```json
{
  "status": "deleted"
}
```

**错误**:

| 状态码 | 条件 |
|--------|------|
| 404 | 记忆不存在 |

**curl 示例**:

```bash
curl -X DELETE http://localhost:8080/v1/memories/550e8400-e29b-41d4-a716-446655440000 \
  -H "X-API-Key: YOUR_API_KEY"
```

---

### GET /v1/memories

分页列出记忆，支持多维度过滤和排序。

**认证**: 需要 `X-API-Key`

**Query 参数**:

| 参数 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| limit | integer | 否 | 20 | 每页数量 |
| offset | integer | 否 | 0 | 分页偏移 |
| category | string | 否 | null | 按分类过滤：`profile`, `preferences`, `entities`, `events`, `cases`, `patterns` |
| tier | string | 否 | null | 按层级过滤：`core`, `working`, `peripheral` |
| tags | string | 否 | null | 按标签过滤，逗号分隔（AND 逻辑） |
| memory_type | string | 否 | null | 按类型过滤：`pinned`, `insight`, `session` |
| state | string | 否 | null | 按状态过滤：`active`, `archived`, `deleted`。不传则排除 `deleted` |
| sort | string | 否 | `"created_at"` | 排序字段：`created_at`, `updated_at`, `importance`, `access_count` |
| order | string | 否 | `"desc"` | 排序方向：`asc` 或 `desc` |

**Response** `200 OK`:

```json
{
  "memories": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "content": "User prefers dark mode in all editors",
      "l0_abstract": "User prefers dark mode",
      "l1_overview": "",
      "l2_content": "User prefers dark mode in all editors",
      "category": "preferences",
      "memory_type": "pinned",
      "state": "active",
      "tier": "peripheral",
      "importance": 0.5,
      "confidence": 0.5,
      "access_count": 0,
      "tags": ["preferences"],
      "scope": "global",
      "agent_id": null,
      "session_id": null,
      "tenant_id": "550e8400-e29b-41d4-a716-446655440000",
      "source": null,
      "relations": [],
      "superseded_by": null,
      "invalidated_at": null,
      "created_at": "2025-01-15T10:30:00+00:00",
      "updated_at": "2025-01-15T10:30:00+00:00",
      "last_accessed_at": null,
      "space_id": "",
      "visibility": "global",
      "owner_agent_id": "",
      "provenance": null
    }
  ],
  "total_count": 42,
  "limit": 20,
  "offset": 0
}
```

**curl 示例**:

```bash
# 列出所有 active 的 pinned 记忆，按重要性降序
curl "http://localhost:8080/v1/memories?memory_type=pinned&state=active&sort=importance&order=desc&limit=50" \
  -H "X-API-Key: YOUR_API_KEY"

# 按分类和标签过滤
curl "http://localhost:8080/v1/memories?category=preferences&tags=editor,theme&limit=10" \
  -H "X-API-Key: YOUR_API_KEY"

# 分页
curl "http://localhost:8080/v1/memories?limit=20&offset=40" \
  -H "X-API-Key: YOUR_API_KEY"
```

---

## 三、用户画像

### GET /v1/profile

获取聚合的用户画像，包含静态事实和动态上下文。可选传入查询文本获取相关搜索结果。

**认证**: 需要 `X-API-Key`

**Query 参数**:

| 参数 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| q | string | 否 | `""` | 查询文本，传入后会返回相关记忆搜索结果 |

**Response** `200 OK`:

不带 `q` 参数：

```json
{
  "static_facts": [
    "Senior backend engineer at Stripe",
    "3 years of Rust experience",
    "Speaks Mandarin and English"
  ],
  "dynamic_context": [
    "Working on omem project",
    "Recently switched from VS Code to Zed"
  ],
  "search_results": null
}
```

带 `q` 参数：

```json
{
  "static_facts": [
    "Senior backend engineer at Stripe"
  ],
  "dynamic_context": [
    "Working on omem project"
  ],
  "search_results": [
    {
      "score": 0.85,
      "memory": {
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "content": "User prefers Rust for backend services",
        "category": "preferences",
        "tags": ["rust", "backend"]
      }
    }
  ]
}
```

**curl 示例**:

```bash
# 获取完整画像
curl "http://localhost:8080/v1/profile" \
  -H "X-API-Key: YOUR_API_KEY"

# 带查询的画像
curl "http://localhost:8080/v1/profile?q=programming+languages" \
  -H "X-API-Key: YOUR_API_KEY"
```

---

## 四、Space 空间管理

Space 是记忆的组织单元，支持 personal（个人）、team（团队）、organization（组织）三种类型。

### POST /v1/spaces

创建新 Space。创建者自动成为 Admin 成员。

**认证**: 需要 `X-API-Key`

**Request Body**:

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| name | string | 是 | Space 名称 |
| space_type | string | 是 | 类型：`"personal"`, `"team"`, `"organization"` |
| members | array | 否 | 初始成员列表 |
| members[].user_id | string | 是 | 用户 ID |
| members[].role | string | 是 | 角色：`"admin"`, `"member"`, `"reader"` |

```json
{
  "name": "Backend Team",
  "space_type": "team",
  "members": [
    {"user_id": "user-002", "role": "member"},
    {"user_id": "user-003", "role": "reader"}
  ]
}
```

**Response** `201 Created`:

```json
{
  "id": "team/550e8400-e29b-41d4-a716-446655440000",
  "space_type": "team",
  "name": "Backend Team",
  "owner_id": "YOUR_TENANT_ID",
  "members": [
    {
      "user_id": "YOUR_TENANT_ID",
      "role": "admin",
      "joined_at": "2025-01-15T10:30:00+00:00"
    },
    {
      "user_id": "user-002",
      "role": "member",
      "joined_at": "2025-01-15T10:30:00+00:00"
    },
    {
      "user_id": "user-003",
      "role": "reader",
      "joined_at": "2025-01-15T10:30:00+00:00"
    }
  ],
  "auto_share_rules": [],
  "created_at": "2025-01-15T10:30:00+00:00",
  "updated_at": "2025-01-15T10:30:00+00:00"
}
```

> Space ID 格式为 `{prefix}:{uuid}`，prefix 取决于类型：`personal`, `team`, `org`。

**错误**:

| 状态码 | 条件 |
|--------|------|
| 400 | `name` 为空 |
| 400 | `space_type` 无效 |
| 400 | `members[].role` 无效 |

**curl 示例**:

```bash
curl -X POST http://localhost:8080/v1/spaces \
  -H "Content-Type: application/json" \
  -H "X-API-Key: YOUR_API_KEY" \
  -d '{"name": "Backend Team", "space_type": "team"}'
```

---

### GET /v1/spaces

列出当前用户所属的所有 Space。

**认证**: 需要 `X-API-Key`

**Response** `200 OK`:

```json
[
  {
    "id": "personal/550e8400-e29b-41d4-a716-446655440000",
    "space_type": "personal",
    "name": "my-workspace",
    "owner_id": "550e8400-e29b-41d4-a716-446655440000",
    "members": [
      {
        "user_id": "550e8400-e29b-41d4-a716-446655440000",
        "role": "admin",
        "joined_at": "2025-01-15T10:30:00+00:00"
      }
    ],
    "auto_share_rules": [],
    "created_at": "2025-01-15T10:30:00+00:00",
    "updated_at": "2025-01-15T10:30:00+00:00"
  },
  {
    "id": "team/660e8400-e29b-41d4-a716-446655440000",
    "space_type": "team",
    "name": "Backend Team",
    "owner_id": "550e8400-e29b-41d4-a716-446655440000",
    "members": [],
    "auto_share_rules": [],
    "created_at": "2025-01-16T08:00:00+00:00",
    "updated_at": "2025-01-16T08:00:00+00:00"
  }
]
```

**curl 示例**:

```bash
curl http://localhost:8080/v1/spaces \
  -H "X-API-Key: YOUR_API_KEY"
```

---

### GET /v1/spaces/{id}

获取单个 Space 详情。需要是 Space 的 owner 或成员。

**认证**: 需要 `X-API-Key`

**路径参数**:

| 参数 | 类型 | 说明 |
|------|------|------|
| id | string | Space ID（如 `team/xxx`） |

**Response** `200 OK`:

返回完整的 Space 对象（结构同上）。

**错误**:

| 状态码 | 条件 |
|--------|------|
| 401 | 不是该 Space 的成员 |
| 404 | Space 不存在 |

**curl 示例**:

```bash
curl http://localhost:8080/v1/spaces/team/660e8400-e29b-41d4-a716-446655440000 \
  -H "X-API-Key: YOUR_API_KEY"
```

---

### PUT /v1/spaces/{id}

更新 Space 信息。需要 Admin 权限。

**认证**: 需要 `X-API-Key`

**路径参数**:

| 参数 | 类型 | 说明 |
|------|------|------|
| id | string | Space ID |

**Request Body**:

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| name | string | 否 | 新名称（不能为空字符串） |

```json
{
  "name": "Backend & Infra Team"
}
```

**Response** `200 OK`:

返回更新后的完整 Space 对象。

**错误**:

| 状态码 | 条件 |
|--------|------|
| 400 | `name` 为空字符串 |
| 401 | 没有 Admin 权限 |
| 404 | Space 不存在 |

**curl 示例**:

```bash
curl -X PUT http://localhost:8080/v1/spaces/team/660e8400-e29b-41d4-a716-446655440000 \
  -H "Content-Type: application/json" \
  -H "X-API-Key: YOUR_API_KEY" \
  -d '{"name": "Backend & Infra Team"}'
```

---

### DELETE /v1/spaces/{id}

删除 Space。需要 Admin 权限。

**认证**: 需要 `X-API-Key`

**路径参数**:

| 参数 | 类型 | 说明 |
|------|------|------|
| id | string | Space ID |

**Response** `200 OK`:

```json
{
  "status": "deleted"
}
```

**错误**:

| 状态码 | 条件 |
|--------|------|
| 401 | 没有 Admin 权限 |
| 404 | Space 不存在 |

**curl 示例**:

```bash
curl -X DELETE http://localhost:8080/v1/spaces/team/660e8400-e29b-41d4-a716-446655440000 \
  -H "X-API-Key: YOUR_API_KEY"
```

---

### POST /v1/spaces/{id}/members

添加成员到 Space。需要 Admin 权限。

**认证**: 需要 `X-API-Key`

**路径参数**:

| 参数 | 类型 | 说明 |
|------|------|------|
| id | string | Space ID |

**Request Body**:

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| user_id | string | 是 | 要添加的用户 ID |
| role | string | 是 | 角色：`"admin"`, `"member"`, `"reader"` |

```json
{
  "user_id": "user-004",
  "role": "member"
}
```

**Response** `200 OK`:

返回更新后的完整 Space 对象（包含新成员）。

**错误**:

| 状态码 | 条件 |
|--------|------|
| 400 | 用户已经是成员 |
| 400 | `role` 无效 |
| 401 | 没有 Admin 权限 |
| 404 | Space 不存在 |

**curl 示例**:

```bash
curl -X POST http://localhost:8080/v1/spaces/team/660e8400-e29b-41d4-a716-446655440000/members \
  -H "Content-Type: application/json" \
  -H "X-API-Key: YOUR_API_KEY" \
  -d '{"user_id": "user-004", "role": "member"}'
```

---

### DELETE /v1/spaces/{id}/members/{user_id}

从 Space 移除成员。需要 Admin 权限。不能移除 Space owner。

**认证**: 需要 `X-API-Key`

**路径参数**:

| 参数 | 类型 | 说明 |
|------|------|------|
| id | string | Space ID |
| user_id | string | 要移除的用户 ID |

**Response** `200 OK`:

返回更新后的完整 Space 对象。

**错误**:

| 状态码 | 条件 |
|--------|------|
| 400 | 试图移除 Space owner |
| 401 | 没有 Admin 权限 |
| 404 | Space 不存在 |
| 404 | 成员不存在 |

**curl 示例**:

```bash
curl -X DELETE http://localhost:8080/v1/spaces/team/660e8400-e29b-41d4-a716-446655440000/members/user-004 \
  -H "X-API-Key: YOUR_API_KEY"
```

---

### PUT /v1/spaces/{id}/members/{user_id}

更新成员角色。需要 Admin 权限。

**认证**: 需要 `X-API-Key`

**路径参数**:

| 参数 | 类型 | 说明 |
|------|------|------|
| id | string | Space ID |
| user_id | string | 目标用户 ID |

**Request Body**:

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| role | string | 是 | 新角色：`"admin"`, `"member"`, `"reader"` |

```json
{
  "role": "admin"
}
```

**Response** `200 OK`:

返回更新后的完整 Space 对象。

**错误**:

| 状态码 | 条件 |
|--------|------|
| 400 | `role` 无效 |
| 401 | 没有 Admin 权限 |
| 404 | Space 不存在 |
| 404 | 成员不存在 |

**curl 示例**:

```bash
curl -X PUT http://localhost:8080/v1/spaces/team/660e8400-e29b-41d4-a716-446655440000/members/user-004 \
  -H "Content-Type: application/json" \
  -H "X-API-Key: YOUR_API_KEY" \
  -d '{"role": "admin"}'
```

---

## 五、记忆分享

### POST /v1/memories/{id}/share

将记忆分享到目标 Space。在目标 Space 中创建一份副本，并记录 provenance（来源追踪）。

**认证**: 需要 `X-API-Key`

**路径参数**:

| 参数 | 类型 | 说明 |
|------|------|------|
| id | string | 要分享的记忆 UUID |

**Request Body**:

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| target_space | string | 是 | 目标 Space ID |
| note | string | 否 | 分享备注 |

```json
{
  "target_space": "team/backend",
  "note": "This architecture decision is relevant to the team"
}
```

**Response** `201 Created`:

返回在目标 Space 中创建的记忆副本（完整 Memory 对象），其中 `provenance` 字段记录了来源信息：

```json
{
  "id": "770e8400-e29b-41d4-a716-446655440000",
  "content": "Use hexagonal architecture for new services",
  "l0_abstract": "",
  "l1_overview": "",
  "l2_content": "Use hexagonal architecture for new services",
  "category": "cases",
  "memory_type": "insight",
  "state": "active",
  "tier": "peripheral",
  "importance": 0.7,
  "confidence": 0.8,
  "access_count": 0,
  "tags": ["architecture"],
  "scope": "global",
  "agent_id": "coder",
  "session_id": null,
  "tenant_id": "550e8400-e29b-41d4-a716-446655440000",
  "source": null,
  "relations": [],
  "superseded_by": null,
  "invalidated_at": null,
  "created_at": "2025-01-20T14:00:00+00:00",
  "updated_at": "2025-01-20T14:00:00+00:00",
  "last_accessed_at": null,
  "space_id": "team/backend",
  "visibility": "global",
  "owner_agent_id": "coder",
  "provenance": {
    "shared_from_space": "personal/550e8400-e29b-41d4-a716-446655440000",
    "shared_from_memory": "660e8400-e29b-41d4-a716-446655440000",
    "shared_by_user": "550e8400-e29b-41d4-a716-446655440000",
    "shared_by_agent": "coder",
    "shared_at": "2025-01-20T14:00:00+00:00",
    "original_created_at": "2025-01-10T09:00:00+00:00"
  }
}
```

**错误**:

| 状态码 | 条件 |
|--------|------|
| 400 | `target_space` 为空 |
| 401 | 对目标 Space 没有写权限（Reader 角色不能分享） |
| 404 | 记忆不存在 |
| 404 | 目标 Space 不存在 |

**curl 示例**:

```bash
curl -X POST http://localhost:8080/v1/memories/660e8400-e29b-41d4-a716-446655440000/share \
  -H "Content-Type: application/json" \
  -H "X-API-Key: YOUR_API_KEY" \
  -d '{"target_space": "team/backend"}'
```

---

### POST /v1/memories/{id}/pull

从其他 Space 拉取记忆到个人空间。

**认证**: 需要 `X-API-Key`

**路径参数**:

| 参数 | 类型 | 说明 |
|------|------|------|
| id | string | 要拉取的记忆 UUID |

**Request Body**:

| 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| source_space | string | 是 | - | 来源 Space ID |
| visibility | string | 否 | `"private"` | 拉取后的可见性 |

```json
{
  "source_space": "team/backend",
  "visibility": "private"
}
```

**Response** `201 Created`:

返回在个人空间中创建的记忆副本（完整 Memory 对象，含 provenance）。

**错误**:

| 状态码 | 条件 |
|--------|------|
| 400 | `source_space` 为空 |
| 401 | 对来源 Space 没有访问权限 |
| 404 | 记忆不存在 |
| 404 | 来源 Space 不存在 |

**curl 示例**:

```bash
curl -X POST http://localhost:8080/v1/memories/770e8400-e29b-41d4-a716-446655440000/pull \
  -H "Content-Type: application/json" \
  -H "X-API-Key: YOUR_API_KEY" \
  -d '{"source_space": "team/backend", "visibility": "private"}'
```

---

### POST /v1/memories/{id}/unshare

撤销分享，删除目标 Space 中的记忆副本。只有原始分享者或 Space Admin 可以操作。

**认证**: 需要 `X-API-Key`

**路径参数**:

| 参数 | 类型 | 说明 |
|------|------|------|
| id | string | 原始记忆 UUID |

**Request Body**:

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| target_space | string | 是 | 目标 Space ID |

```json
{
  "target_space": "team/backend"
}
```

**Response** `200 OK`:

```json
{
  "status": "unshared"
}
```

**错误**:

| 状态码 | 条件 |
|--------|------|
| 400 | `target_space` 为空 |
| 401 | 不是原始分享者且不是 Admin |
| 401 | 对目标 Space 没有写权限 |
| 404 | 目标 Space 中没有该记忆的副本 |
| 404 | 目标 Space 不存在 |

**curl 示例**:

```bash
curl -X POST http://localhost:8080/v1/memories/660e8400-e29b-41d4-a716-446655440000/unshare \
  -H "Content-Type: application/json" \
  -H "X-API-Key: YOUR_API_KEY" \
  -d '{"target_space": "team/backend"}'
```

---

### POST /v1/memories/batch-share

批量分享多条记忆到目标 Space。部分失败不影响其他记忆的分享。

**认证**: 需要 `X-API-Key`

**Request Body**:

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| memory_ids | string[] | 是 | 要分享的记忆 UUID 列表 |
| target_space | string | 是 | 目标 Space ID |

```json
{
  "memory_ids": [
    "550e8400-e29b-41d4-a716-446655440000",
    "660e8400-e29b-41d4-a716-446655440000",
    "770e8400-e29b-41d4-a716-446655440000"
  ],
  "target_space": "team/backend"
}
```

**Response** `200 OK`:

```json
{
  "succeeded": [
    {
      "id": "new-copy-uuid-1",
      "content": "...",
      "space_id": "team/backend",
      "provenance": { "..." : "..." }
    },
    {
      "id": "new-copy-uuid-2",
      "content": "...",
      "space_id": "team/backend",
      "provenance": { "..." : "..." }
    }
  ],
  "failed": [
    {
      "memory_id": "770e8400-e29b-41d4-a716-446655440000",
      "error": "not found: memory 770e8400-e29b-41d4-a716-446655440000"
    }
  ]
}
```

**错误**:

| 状态码 | 条件 |
|--------|------|
| 400 | `memory_ids` 为空 |
| 400 | `target_space` 为空 |
| 401 | 对目标 Space 没有写权限 |
| 404 | 目标 Space 不存在 |

**curl 示例**:

```bash
curl -X POST http://localhost:8080/v1/memories/batch-share \
  -H "Content-Type: application/json" \
  -H "X-API-Key: YOUR_API_KEY" \
  -d '{
    "memory_ids": ["mem-id-1", "mem-id-2"],
    "target_space": "team/backend"
  }'
```

---

### POST /v1/spaces/{id}/auto-share-rules

创建自动分享规则。当新记忆满足规则条件时，自动分享到该 Space。需要 Admin 权限。

**认证**: 需要 `X-API-Key`

**路径参数**:

| 参数 | 类型 | 说明 |
|------|------|------|
| id | string | 目标 Space ID |

**Request Body**:

| 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| source_space | string | 是 | - | 来源 Space ID |
| categories | string[] | 否 | `[]` | 匹配的分类列表（空表示不限） |
| tags | string[] | 否 | `[]` | 匹配的标签列表（空表示不限，OR 逻辑） |
| min_importance | float | 否 | `0.0` | 最低重要性阈值 |
| require_approval | boolean | 否 | `false` | 是否需要审批（true 时不会自动分享） |

```json
{
  "source_space": "personal/user-001",
  "categories": ["cases", "patterns"],
  "tags": ["architecture"],
  "min_importance": 0.7,
  "require_approval": false
}
```

**Response** `201 Created`:

```json
{
  "id": "880e8400-e29b-41d4-a716-446655440000",
  "source_space": "personal/user-001",
  "categories": ["cases", "patterns"],
  "tags": ["architecture"],
  "min_importance": 0.7,
  "require_approval": false,
  "created_at": "2025-01-20T14:00:00+00:00"
}
```

**错误**:

| 状态码 | 条件 |
|--------|------|
| 400 | `source_space` 为空 |
| 401 | 没有 Admin 权限 |
| 404 | Space 不存在 |

**curl 示例**:

```bash
curl -X POST http://localhost:8080/v1/spaces/team/backend/auto-share-rules \
  -H "Content-Type: application/json" \
  -H "X-API-Key: YOUR_API_KEY" \
  -d '{
    "source_space": "personal/user-001",
    "categories": ["cases"],
    "min_importance": 0.7
  }'
```

---

### GET /v1/spaces/{id}/auto-share-rules

列出 Space 的所有自动分享规则。需要是 Space 成员。

**认证**: 需要 `X-API-Key`

**路径参数**:

| 参数 | 类型 | 说明 |
|------|------|------|
| id | string | Space ID |

**Response** `200 OK`:

```json
[
  {
    "id": "880e8400-e29b-41d4-a716-446655440000",
    "source_space": "personal/user-001",
    "categories": ["cases", "patterns"],
    "tags": ["architecture"],
    "min_importance": 0.7,
    "require_approval": false,
    "created_at": "2025-01-20T14:00:00+00:00"
  }
]
```

**错误**:

| 状态码 | 条件 |
|--------|------|
| 401 | 不是 Space 成员 |
| 404 | Space 不存在 |

**curl 示例**:

```bash
curl http://localhost:8080/v1/spaces/team/backend/auto-share-rules \
  -H "X-API-Key: YOUR_API_KEY"
```

---

### DELETE /v1/spaces/{id}/auto-share-rules/{rule_id}

删除自动分享规则。需要 Admin 权限。

**认证**: 需要 `X-API-Key`

**路径参数**:

| 参数 | 类型 | 说明 |
|------|------|------|
| id | string | Space ID |
| rule_id | string | 规则 UUID |

**Response** `200 OK`:

```json
{
  "status": "deleted"
}
```

**错误**:

| 状态码 | 条件 |
|--------|------|
| 401 | 没有 Admin 权限 |
| 404 | Space 不存在 |
| 404 | 规则不存在 |

**curl 示例**:

```bash
curl -X DELETE http://localhost:8080/v1/spaces/team/backend/auto-share-rules/880e8400-e29b-41d4-a716-446655440000 \
  -H "X-API-Key: YOUR_API_KEY"
```

---

## 六、统计分析

### GET /v1/stats

获取记忆的综合统计信息，包含按类型/分类/层级/状态/Space/可见性/Agent 的分布，以及时间线数据。

**认证**: 需要 `X-API-Key`

**Query 参数**:

| 参数 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| days | integer | 否 | 30 | 时间线统计的天数范围 |
| space | string | 否 | null | 按 Space 过滤。不传则聚合所有 Space |

**Response** `200 OK`:

```json
{
  "total": 156,
  "by_type": {
    "pinned": 23,
    "insight": 98,
    "session": 35
  },
  "by_category": {
    "preferences": 34,
    "entities": 45,
    "events": 28,
    "cases": 22,
    "patterns": 18,
    "profile": 9
  },
  "by_tier": {
    "core": 12,
    "working": 45,
    "peripheral": 99
  },
  "by_state": {
    "active": 150,
    "archived": 6
  },
  "by_space": {
    "personal/user-001": 80,
    "team/backend": 56,
    "default": 20
  },
  "by_visibility": {
    "global": 140,
    "private": 16
  },
  "by_agent": {
    "coder": 90,
    "writer": 40,
    "": 26
  },
  "timeline": [
    {
      "date": "2025-01-20",
      "count": 8,
      "by_type": {
        "insight": 5,
        "session": 3
      }
    },
    {
      "date": "2025-01-19",
      "count": 12,
      "by_type": {
        "insight": 7,
        "pinned": 2,
        "session": 3
      }
    }
  ],
  "avg_importance": 0.62,
  "avg_confidence": 0.71,
  "total_access_count": 1234
}
```

**curl 示例**:

```bash
# 全局统计
curl "http://localhost:8080/v1/stats?days=30" \
  -H "X-API-Key: YOUR_API_KEY"

# 按 Space 过滤
curl "http://localhost:8080/v1/stats?space=team/backend" \
  -H "X-API-Key: YOUR_API_KEY"
```

---

### GET /v1/stats/spaces

获取所有 Space 的详细统计信息。

**认证**: 需要 `X-API-Key`

**Response** `200 OK`:

```json
{
  "spaces": [
    {
      "space_id": "personal/user-001",
      "space_type": "personal",
      "name": "My Workspace",
      "owner_id": "user-001",
      "memory_count": 80,
      "agent_count": 2,
      "tier_distribution": {
        "core": 5,
        "working": 25,
        "peripheral": 50
      },
      "top_categories": [
        {"category": "preferences", "count": 30},
        {"category": "entities", "count": 25},
        {"category": "events", "count": 15}
      ],
      "last_activity": "2025-01-20T14:00:00+00:00",
      "shared_in_count": 3,
      "member_count": 1,
      "members": [
        {"user_id": "user-001", "role": "admin"}
      ]
    }
  ],
  "total_spaces": 2
}
```

**curl 示例**:

```bash
curl http://localhost:8080/v1/stats/spaces \
  -H "X-API-Key: YOUR_API_KEY"
```

---

### GET /v1/stats/sharing

获取分享活动的统计和流向分析。

**认证**: 需要 `X-API-Key`

**Query 参数**:

| 参数 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| days | integer | 否 | 30 | 统计天数范围 |

**Response** `200 OK`:

```json
{
  "summary": {
    "total_shares": 45,
    "total_pulls": 12,
    "total_unshares": 3,
    "unique_sharers": 5
  },
  "recent_activity": [
    {
      "id": "evt-001",
      "action": "share",
      "memory_id": "mem-001",
      "from_space": "personal/user-001",
      "to_space": "team/backend",
      "user_id": "user-001",
      "agent_id": "coder",
      "content_preview": "Use hexagonal architecture for...",
      "timestamp": "2025-01-20T14:00:00+00:00"
    }
  ],
  "flow_graph": {
    "nodes": ["personal/user-001", "team/backend", "org/acme"],
    "edges": [
      {"from": "personal/user-001", "to": "team/backend", "count": 30},
      {"from": "personal/user-002", "to": "team/backend", "count": 15}
    ]
  },
  "timeline": [
    {
      "date": "2025-01-20",
      "shares": 5,
      "pulls": 2
    }
  ]
}
```

**curl 示例**:

```bash
curl "http://localhost:8080/v1/stats/sharing?days=7" \
  -H "X-API-Key: YOUR_API_KEY"
```

---

### GET /v1/stats/agents

获取各 Agent 的活动统计。

**认证**: 需要 `X-API-Key`

**Response** `200 OK`:

```json
{
  "agents": [
    {
      "agent_id": "coder",
      "total_memories": 90,
      "memories_by_space": {
        "personal/user-001": 60,
        "team/backend": 30
      },
      "top_categories": [
        {"category": "entities", "count": 40},
        {"category": "cases", "count": 30},
        {"category": "patterns", "count": 20}
      ],
      "last_active": "2025-01-20T14:00:00+00:00",
      "share_count": 15
    },
    {
      "agent_id": "writer",
      "total_memories": 40,
      "memories_by_space": {
        "personal/user-001": 40
      },
      "top_categories": [
        {"category": "events", "count": 20}
      ],
      "last_active": "2025-01-19T10:00:00+00:00",
      "share_count": 0
    }
  ],
  "total_agents": 2
}
```

**curl 示例**:

```bash
curl http://localhost:8080/v1/stats/agents \
  -H "X-API-Key: YOUR_API_KEY"
```

---

### GET /v1/stats/tags

获取标签使用统计，包括跨 Space 标签检测。

**认证**: 需要 `X-API-Key`

**Query 参数**:

| 参数 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| limit | integer | 否 | 20 | 返回的标签数量上限 |
| min_count | integer | 否 | 1 | 最低使用次数 |
| space | string | 否 | null | 按 Space 过滤 |

**Response** `200 OK`:

```json
{
  "tags": [
    {"name": "rust", "count": 45},
    {"name": "architecture", "count": 32},
    {"name": "preferences", "count": 28},
    {"name": "debugging", "count": 15}
  ],
  "total_unique_tags": 67,
  "total_tag_usages": 312,
  "cross_space_tags": ["rust", "architecture", "debugging"]
}
```

> `cross_space_tags` 列出在 2 个或更多 Space 中都出现的标签。

**curl 示例**:

```bash
curl "http://localhost:8080/v1/stats/tags?limit=10&min_count=3" \
  -H "X-API-Key: YOUR_API_KEY"
```

---

### GET /v1/stats/decay

获取单条记忆的衰减曲线和当前强度。用于可视化 Weibull 衰减模型。

**认证**: 需要 `X-API-Key`

**Query 参数**:

| 参数 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| memory_id | string | 是 | - | 记忆 UUID |
| points | integer | 否 | 90 | 衰减曲线的数据点数量（天数） |

**Response** `200 OK`:

```json
{
  "memory_id": "550e8400-e29b-41d4-a716-446655440000",
  "tier": "working",
  "importance": 0.75,
  "confidence": 0.85,
  "access_count": 8,
  "last_accessed_hours_ago": 12.5,
  "current_strength": 0.72,
  "composite_score": 0.72,
  "decay_params": {
    "beta": 1.2,
    "half_life_days": 18.5,
    "lambda": 0.0375,
    "floor": 0.2,
    "importance_modulation": 0.5
  },
  "decay_curve": [
    {"day": 0, "strength": 1.0},
    {"day": 1, "strength": 0.96},
    {"day": 7, "strength": 0.78},
    {"day": 30, "strength": 0.42},
    {"day": 89, "strength": 0.2}
  ],
  "promotion_thresholds": {
    "to_working": {"access_count": 3, "composite": 0.4},
    "to_core": {"access_count": 10, "composite": 0.7, "importance": 0.8}
  },
  "demotion_thresholds": {
    "core_to_working": {"composite": 0.15},
    "working_to_peripheral": {"composite": 0.15}
  }
}
```

**错误**:

| 状态码 | 条件 |
|--------|------|
| 400 | `memory_id` 为空 |
| 404 | 记忆不存在 |

**curl 示例**:

```bash
curl "http://localhost:8080/v1/stats/decay?memory_id=550e8400-e29b-41d4-a716-446655440000&points=90" \
  -H "X-API-Key: YOUR_API_KEY"
```

---

### GET /v1/stats/relations

获取记忆关系图谱数据（节点 + 边），用于可视化。包含跨 Space 关系和 provenance 关系。

**认证**: 需要 `X-API-Key`

**Query 参数**:

| 参数 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| limit | integer | 否 | 100 | 最大节点数 |
| min_importance | float | 否 | 0.0 | 最低重要性过滤 |

**Response** `200 OK`:

```json
{
  "nodes": [
    {
      "id": "mem-001",
      "label": "User prefers dark mode",
      "category": "preferences",
      "tier": "core",
      "importance": 0.85,
      "access_count": 12,
      "memory_type": "insight",
      "space_id": "personal/user-001"
    },
    {
      "id": "mem-002",
      "label": "Switched to Catppuccin theme",
      "category": "preferences",
      "tier": "working",
      "importance": 0.7,
      "access_count": 5,
      "memory_type": "insight",
      "space_id": "personal/user-001"
    }
  ],
  "edges": [
    {
      "source": "mem-002",
      "target": "mem-001",
      "relation_type": "supersedes",
      "context_label": "updated theme preference",
      "cross_space": false
    },
    {
      "source": "mem-original",
      "target": "mem-shared-copy",
      "relation_type": "shared_from",
      "context_label": "personal/user-001 → team/backend",
      "cross_space": true
    }
  ],
  "total_nodes": 2,
  "total_edges": 2
}
```

> `relation_type` 可能的值：`supersedes`, `contextualizes`, `supports`, `contradicts`, `shared_from`。

**curl 示例**:

```bash
curl "http://localhost:8080/v1/stats/relations?limit=50&min_importance=0.5" \
  -H "X-API-Key: YOUR_API_KEY"
```

---

### GET /v1/stats/config

获取系统配置信息，包括衰减参数、检索管线配置、准入控制阈值等。

**认证**: 需要 `X-API-Key`

**Response** `200 OK`:

```json
{
  "decay": {
    "half_life_days": 14.0,
    "importance_modulation": 0.5,
    "recency_weight": 0.4,
    "frequency_weight": 0.3,
    "intrinsic_weight": 0.3,
    "tiers": {
      "core": {"beta": 0.8, "floor": 0.4},
      "working": {"beta": 1.2, "floor": 0.2},
      "peripheral": {"beta": 1.8, "floor": 0.05}
    }
  },
  "promotion": {
    "peripheral_to_working": {"min_access_count": 3, "min_composite": 0.4},
    "working_to_core": {"min_access_count": 10, "min_composite": 0.7, "min_importance": 0.8}
  },
  "demotion": {
    "core_to_working": {"max_composite": 0.15},
    "working_to_peripheral": {"max_composite": 0.15}
  },
  "retrieval": {
    "stages": [
      "parallel_search", "rrf_fusion", "rrf_normalize", "min_score_filter",
      "topk_cap", "cross_encoder_rerank", "bm25_floor", "decay_boost",
      "importance_weight", "length_normalization", "hard_cutoff", "mmr_diversity"
    ],
    "default_min_score": 0.3,
    "rrf_k": 60,
    "vector_weight": 0.7,
    "bm25_weight": 0.3
  },
  "admission": {
    "presets": {
      "balanced": {"reject": 0.45, "admit": 0.60},
      "conservative": {"reject": 0.52, "admit": 0.68},
      "high_recall": {"reject": 0.34, "admit": 0.52}
    },
    "weights": {
      "utility": 0.1,
      "confidence": 0.1,
      "novelty": 0.1,
      "recency": 0.1,
      "type_prior": 0.6
    }
  },
  "spaces": {
    "search_weights": {"personal": 1.0, "team": 0.8, "organization": 0.6},
    "max_spaces_per_user": 20,
    "max_members_per_team": 50
  },
  "categories": ["profile", "preferences", "entities", "events", "cases", "patterns"],
  "tiers": ["core", "working", "peripheral"],
  "memory_types": ["pinned", "insight", "session"],
  "states": ["active", "archived", "deleted"],
  "relation_types": ["supersedes", "contextualizes", "supports", "contradicts"]
}
```

**curl 示例**:

```bash
curl http://localhost:8080/v1/stats/config \
  -H "X-API-Key: YOUR_API_KEY"
```

---

## 七、文件上传

### POST /v1/files

上传文件进行处理和记忆提取。支持 PDF、图片、视频、代码文件和纯文本。文件会被异步处理，拆分为多个 chunk 并存储为独立记忆。

**认证**: 需要 `X-API-Key`

**Request**: `Content-Type: multipart/form-data`

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| file | file | 是 | 上传的文件（最大 50MB） |

**Response** `202 Accepted`:

```json
{
  "task_id": "550e8400-e29b-41d4-a716-446655440000",
  "filename": "architecture.pdf",
  "content_type": "Pdf",
  "chunks_created": 12
}
```

> 文件在后台异步处理。每个 chunk 会被嵌入并存储为独立记忆，带有 `file:{filename}` 和 `chunk_type:{type}` 标签。

**错误**:

| 状态码 | 条件 |
|--------|------|
| 400 | 没有 `file` 字段 |
| 400 | 文件超过 50MB |

**curl 示例**:

```bash
curl -X POST http://localhost:8080/v1/files \
  -H "X-API-Key: YOUR_API_KEY" \
  -F "file=@document.pdf"
```

---

## 八、GitHub 连接器

### POST /v1/connectors/github/webhook

接收 GitHub Webhook 事件。不需要 API Key 认证，使用 Webhook 签名验证。

**认证**: 不需要 `X-API-Key`（通过 Webhook 签名验证）

**请求头**:

| Header | 必填 | 说明 |
|--------|------|------|
| X-GitHub-Event | 是 | 事件类型（`push`, `issues`, `pull_request` 等） |
| X-Hub-Signature-256 | 条件 | HMAC-SHA256 签名（配置了 `OMEM_GITHUB_WEBHOOK_SECRET` 时必填） |
| X-Tenant-Id | 否 | 租户 ID（默认 `"default"`） |

**Request Body**: GitHub Webhook 标准 JSON payload

**支持的事件类型**:

| 事件 | 创建的记忆 |
|------|-----------|
| `push` | 提交信息 + 变更文件 |
| `issues` | Issue 创建/更新 |
| `issue_comment` | Issue 评论 |
| `pull_request` | PR 创建/更新 |
| `pull_request_review` | PR Review |

**Response** `200 OK`:

```json
{
  "event_type": "push",
  "memories_created": 3
}
```

**错误**:

| 状态码 | 条件 |
|--------|------|
| 400 | Webhook payload 格式无效 |
| 401 | 签名验证失败（配置了 secret 时） |

---

### POST /v1/connectors/github/connect

注册 GitHub Webhook。在指定仓库上创建 Webhook，用于实时同步事件。

**认证**: 需要 `X-API-Key`

**Request Body**:

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| access_token | string | 是 | GitHub Personal Access Token |
| repo | string | 是 | 仓库全名（如 `"owner/repo"`） |
| webhook_url | string | 是 | Webhook 回调 URL |

```json
{
  "access_token": "ghp_xxxxxxxxxxxxxxxxxxxx",
  "repo": "myorg/myrepo",
  "webhook_url": "https://omem.example.com/v1/connectors/github/webhook"
}
```

**Response** `200 OK`:

```json
{
  "status": "connected",
  "repo": "myorg/myrepo",
  "webhook_id": 123456789
}
```

**错误**:

| 状态码 | 条件 |
|--------|------|
| 400 | `repo` 为空 |
| 400 | `access_token` 为空 |

**curl 示例**:

```bash
curl -X POST http://localhost:8080/v1/connectors/github/connect \
  -H "Content-Type: application/json" \
  -H "X-API-Key: YOUR_API_KEY" \
  -d '{
    "access_token": "ghp_xxxxxxxxxxxxxxxxxxxx",
    "repo": "myorg/myrepo",
    "webhook_url": "https://omem.example.com/v1/connectors/github/webhook"
  }'
```

---

## 九、健康检查

### GET /health

健康检查端点。

**认证**: 不需要

**Response** `200 OK`:

```json
{
  "status": "ok"
}
```

**curl 示例**:

```bash
curl http://localhost:8080/health
```

---

## 附录

### A. Memory 完整字段说明

Memory 对象包含 28 个字段：

| 字段 | 类型 | 可空 | 说明 |
|------|------|------|------|
| id | string | 否 | UUID v4 唯一标识 |
| content | string | 否 | 记忆原始内容 |
| l0_abstract | string | 否 | 一行摘要（LLM 生成） |
| l1_overview | string | 否 | 段落级概述（LLM 生成） |
| l2_content | string | 否 | 完整详细内容 |
| category | string | 否 | 分类，见枚举值 |
| memory_type | string | 否 | 记忆类型，见枚举值 |
| state | string | 否 | 状态，见枚举值 |
| tier | string | 否 | 层级，见枚举值 |
| importance | float | 否 | 重要性分数 (0.0~1.0) |
| confidence | float | 否 | 置信度分数 (0.0~1.0) |
| access_count | integer | 否 | 访问次数 |
| tags | string[] | 否 | 标签列表 |
| scope | string | 否 | 作用域（默认 `"global"`） |
| agent_id | string | 是 | 创建该记忆的 Agent ID |
| session_id | string | 是 | 关联的会话 ID |
| tenant_id | string | 否 | 所属租户 ID |
| source | string | 是 | 来源标识（如 `"manual"`, `"file_upload:doc.pdf"`） |
| relations | array | 否 | 关系列表，见 MemoryRelation |
| superseded_by | string | 是 | 被哪条记忆取代（UUID） |
| invalidated_at | string | 是 | 失效时间（ISO 8601） |
| created_at | string | 否 | 创建时间（ISO 8601） |
| updated_at | string | 否 | 更新时间（ISO 8601） |
| last_accessed_at | string | 是 | 最后访问时间（ISO 8601） |
| space_id | string | 否 | 所属 Space ID |
| visibility | string | 否 | 可见性：`"global"`, `"private"`, `"shared:<group-id>"` |
| owner_agent_id | string | 否 | 拥有者 Agent ID |
| provenance | object | 是 | 来源追踪信息（分享时填充） |

**MemoryRelation 结构**:

| 字段 | 类型 | 可空 | 说明 |
|------|------|------|------|
| relation_type | string | 否 | 关系类型，见枚举值 |
| target_id | string | 否 | 目标记忆 UUID |
| context_label | string | 是 | 关系上下文描述 |

**Provenance 结构**:

| 字段 | 类型 | 说明 |
|------|------|------|
| shared_from_space | string | 来源 Space ID |
| shared_from_memory | string | 来源记忆 UUID |
| shared_by_user | string | 分享者用户 ID |
| shared_by_agent | string | 分享者 Agent ID |
| shared_at | string | 分享时间（ISO 8601） |
| original_created_at | string | 原始记忆创建时间（ISO 8601） |

### B. 枚举值参考

**Category（分类）**:

| 值 | 说明 | 行为 |
|----|------|------|
| `profile` | 用户画像 | 始终合并 |
| `preferences` | 偏好设置 | 时间版本化 |
| `entities` | 实体信息 | 时间版本化 |
| `events` | 事件记录 | 仅追加 |
| `cases` | 案例经验 | 仅追加 |
| `patterns` | 行为模式 | 支持合并 |

**MemoryType（记忆类型）**:

| 值 | 说明 |
|----|------|
| `pinned` | 用户手动创建，不会被自动删除 |
| `insight` | LLM 从对话中提取的洞察 |
| `session` | 原始会话消息 |

**MemoryState（状态）**:

| 值 | 说明 |
|----|------|
| `active` | 活跃状态 |
| `archived` | 已归档 |
| `deleted` | 已删除（软删除） |

**Tier（层级）**:

| 值 | 说明 | 衰减速度 |
|----|------|----------|
| `core` | 核心记忆，频繁访问 | 慢 |
| `working` | 工作记忆，中等访问 | 中 |
| `peripheral` | 边缘记忆，较少访问 | 快 |

**RelationType（关系类型）**:

| 值 | 说明 |
|----|------|
| `supersedes` | 取代（新记忆替换旧记忆） |
| `contextualizes` | 提供上下文 |
| `supports` | 支持/佐证 |
| `contradicts` | 矛盾/冲突 |

**SpaceType（空间类型）**:

| 值 | 说明 | ID 前缀 |
|----|------|---------|
| `personal` | 个人空间 | `personal/` |
| `team` | 团队空间 | `team/` |
| `organization` | 组织空间 | `org/` |

**MemberRole（成员角色）**:

| 值 | 权限 |
|----|------|
| `admin` | 完全控制（增删改查 + 管理成员 + 管理规则） |
| `member` | 读写（增删改查记忆 + 分享） |
| `reader` | 只读（查看记忆，不能分享到该 Space） |

**SharingAction（分享动作）**:

| 值 | 说明 |
|----|------|
| `share` | 分享记忆到 Space |
| `pull` | 从 Space 拉取记忆 |
| `unshare` | 撤销分享 |
| `batch_share` | 批量分享 |

**TenantStatus（租户状态）**:

| 值 | 说明 |
|----|------|
| `active` | 活跃 |
| `suspended` | 已停用 |
| `deleted` | 已删除 |

### C. 错误码参考

| HTTP 状态码 | error.code | OmemError 变体 | 说明 |
|-------------|------------|----------------|------|
| 400 | `validation_error` | `Validation(msg)` | 请求参数验证失败 |
| 401 | `unauthorized` | `Unauthorized(msg)` | 认证失败或权限不足 |
| 404 | `not_found` | `NotFound(msg)` | 资源不存在 |
| 429 | `rate_limited` | `RateLimited` | 请求频率超限 |
| 500 | `internal_error` | `Storage(msg)` | 存储层错误 |
| 500 | `internal_error` | `Embedding(msg)` | 嵌入服务错误 |
| 500 | `internal_error` | `Llm(msg)` | LLM 服务错误 |
| 500 | `internal_error` | `Internal(msg)` | 其他内部错误 |

错误响应格式：

```json
{
  "error": {
    "code": "validation_error",
    "message": "validation error: content cannot be empty"
  }
}
```
