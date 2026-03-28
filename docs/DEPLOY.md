# omem — Deployment Guide

## Table of Contents

1. [Docker Quick Start (Development)](#1-docker-quick-start-development)
2. [Production Docker Deployment](#2-production-docker-deployment)
3. [AWS Deployment (ECS Fargate + S3)](#3-aws-deployment-ecs-fargate--s3)
4. [Environment Variables Reference](#4-environment-variables-reference)
5. [Monitoring & Observability](#5-monitoring--observability)

---

## 1. Docker Quick Start (Development)

The development setup uses MinIO as a local S3 replacement.

### Prerequisites

- Docker Engine 20+
- Docker Compose v2

### Start

```bash
# Clone the repository
git clone https://github.com/ourmem/omem.git
cd omem

# Copy environment file
cp .env.example .env

# Start services (omem-server + MinIO)
docker-compose up -d
```

This starts:

| Service | Port | Description |
|---------|------|-------------|
| `omem-server` | 8080 | omem REST API |
| `minio` | 9000 | S3-compatible storage |
| `minio` | 9001 | MinIO web console |

### Verify

```bash
# Health check
curl http://localhost:8080/health
# → {"status":"ok"}

# Create a tenant
curl -X POST http://localhost:8080/v1/tenants \
  -H "Content-Type: application/json" \
  -d '{"name":"dev"}'
# → {"id":"...","api_key":"...","status":"active"}
```

### MinIO Console

Open http://localhost:9001 in your browser. Login with `minioadmin` / `minioadmin`.

### docker-compose.yml

```yaml
services:
  omem-server:
    build: .
    ports:
      - "8080:8080"
    env_file: .env
    depends_on:
      minio:
        condition: service_started
    environment:
      AWS_ENDPOINT_URL: http://minio:9000
      AWS_ACCESS_KEY_ID: minioadmin
      AWS_SECRET_ACCESS_KEY: minioadmin
      AWS_REGION: us-east-1
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 30s
      timeout: 5s

  minio:
    image: minio/minio:latest
    command: server /data --console-address ":9001"
    ports:
      - "9000:9000"
      - "9001:9001"
    environment:
      MINIO_ROOT_USER: minioadmin
      MINIO_ROOT_PASSWORD: minioadmin
    volumes:
      - minio-data:/data

volumes:
  minio-data:
```

---

## 2. Production Docker Deployment

For production, use real AWS S3 instead of MinIO.

### Setup

```bash
# Create production .env
cat > .env << 'EOF'
OMEM_PORT=8080
OMEM_LOG_LEVEL=info
OMEM_S3_BUCKET=your-omem-bucket

# AWS credentials (or use IAM role)
AWS_REGION=us-east-1
# AWS_ACCESS_KEY_ID=...      # Only if not using IAM role
# AWS_SECRET_ACCESS_KEY=...  # Only if not using IAM role

# Embedding (choose one)
OMEM_EMBED_PROVIDER=bedrock
# Or:
# OMEM_EMBED_PROVIDER=openai-compatible
# OMEM_EMBED_API_KEY=sk-xxx
# OMEM_EMBED_BASE_URL=https://api.openai.com
# OMEM_EMBED_MODEL=text-embedding-3-small

# LLM for smart extraction (optional)
OMEM_LLM_PROVIDER=openai-compatible
OMEM_LLM_API_KEY=sk-xxx
OMEM_LLM_BASE_URL=https://api.openai.com
OMEM_LLM_MODEL=gpt-4o-mini
EOF

# Start with production compose
docker-compose -f docker-compose.prod.yml up -d
```

### docker-compose.prod.yml

```yaml
services:
  omem-server:
    build: .
    ports:
      - "8080:8080"
    env_file: .env
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 30s
      timeout: 5s
```

### Pre-create S3 Bucket

```bash
aws s3 mb s3://your-omem-bucket --region us-east-1
```

LanceDB will automatically create the necessary table directories under `s3://your-omem-bucket/omem/`.

---

## 3. AWS Deployment (ECS Fargate + S3)

### Architecture

```
Internet → ALB (port 80/443) → ECS Fargate → S3 (LanceDB storage)
                                    ↓
                              CloudWatch Logs
```

### Step 1: Create ECR Repository

```bash
aws ecr create-repository --repository-name omem-server --region us-east-1
```

### Step 2: Build and Push Docker Image

```bash
# Login to ECR
aws ecr get-login-password --region us-east-1 | \
  docker login --username AWS --password-stdin ACCOUNT_ID.dkr.ecr.us-east-1.amazonaws.com

# Build and push
docker build -t omem-server .
docker tag omem-server:latest ACCOUNT_ID.dkr.ecr.us-east-1.amazonaws.com/omem-server:latest
docker push ACCOUNT_ID.dkr.ecr.us-east-1.amazonaws.com/omem-server:latest
```

### Step 3: Create IAM Task Role

The ECS task needs S3 access and optionally Bedrock access:

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": [
        "s3:GetObject",
        "s3:PutObject",
        "s3:DeleteObject",
        "s3:ListBucket"
      ],
      "Resource": [
        "arn:aws:s3:::your-omem-bucket",
        "arn:aws:s3:::your-omem-bucket/*"
      ]
    },
    {
      "Effect": "Allow",
      "Action": [
        "bedrock:InvokeModel"
      ],
      "Resource": "*",
      "Condition": {
        "StringEquals": {
          "aws:RequestedRegion": "us-east-1"
        }
      }
    }
  ]
}
```

### Step 4: Create ECS Task Definition

```json
{
  "family": "omem-server",
  "networkMode": "awsvpc",
  "requiresCompatibilities": ["FARGATE"],
  "cpu": "512",
  "memory": "1024",
  "executionRoleArn": "arn:aws:iam::ACCOUNT_ID:role/ecsTaskExecutionRole",
  "taskRoleArn": "arn:aws:iam::ACCOUNT_ID:role/omem-task-role",
  "containerDefinitions": [
    {
      "name": "omem-server",
      "image": "ACCOUNT_ID.dkr.ecr.us-east-1.amazonaws.com/omem-server:latest",
      "portMappings": [
        {
          "containerPort": 8080,
          "protocol": "tcp"
        }
      ],
      "environment": [
        {"name": "OMEM_PORT", "value": "8080"},
        {"name": "OMEM_LOG_LEVEL", "value": "info"},
        {"name": "OMEM_S3_BUCKET", "value": "your-omem-bucket"},
        {"name": "AWS_REGION", "value": "us-east-1"},
        {"name": "OMEM_EMBED_PROVIDER", "value": "bedrock"}
      ],
      "secrets": [
        {
          "name": "OMEM_LLM_API_KEY",
          "valueFrom": "arn:aws:secretsmanager:us-east-1:ACCOUNT_ID:secret:omem/llm-api-key"
        }
      ],
      "healthCheck": {
        "command": ["CMD-SHELL", "curl -f http://localhost:8080/health || exit 1"],
        "interval": 30,
        "timeout": 5,
        "retries": 3
      },
      "logConfiguration": {
        "logDriver": "awslogs",
        "options": {
          "awslogs-group": "/ecs/omem-server",
          "awslogs-region": "us-east-1",
          "awslogs-stream-prefix": "ecs"
        }
      }
    }
  ]
}
```

### Step 5: Create ECS Service

```bash
# Create cluster
aws ecs create-cluster --cluster-name omem-cluster

# Create service with ALB
aws ecs create-service \
  --cluster omem-cluster \
  --service-name omem-server \
  --task-definition omem-server \
  --desired-count 1 \
  --launch-type FARGATE \
  --network-configuration "awsvpcConfiguration={subnets=[subnet-xxx],securityGroups=[sg-xxx],assignPublicIp=ENABLED}" \
  --load-balancers "targetGroupArn=arn:aws:elasticloadbalancing:...,containerName=omem-server,containerPort=8080"
```

### Resource Sizing

| Workload | CPU | Memory | Estimated Cost |
|----------|-----|--------|----------------|
| Dev/Test | 256 (.25 vCPU) | 512 MB | ~$5/month |
| Small (1 user) | 512 (.5 vCPU) | 1 GB | ~$15/month |
| Medium (10 users) | 1024 (1 vCPU) | 2 GB | ~$35/month |
| Large (100 users) | 2048 (2 vCPU) | 4 GB | ~$70/month |

S3 storage cost: ~$0.023/GB/month (typically <$1/month for most workloads).

---

## 4. Environment Variables Reference

### Server Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `OMEM_PORT` | `8080` | HTTP server port |
| `OMEM_LOG_LEVEL` | `info` | Log level: `trace`, `debug`, `info`, `warn`, `error` |
| `RUST_LOG` | `info` | Alternative log level (Rust standard) |

### Storage

| Variable | Default | Description |
|----------|---------|-------------|
| `OMEM_S3_BUCKET` | `omem-data` | S3 bucket name for LanceDB storage |
| `AWS_ENDPOINT_URL` | _(none)_ | Custom S3 endpoint (for MinIO: `http://minio:9000`) |
| `AWS_ACCESS_KEY_ID` | _(none)_ | AWS access key (not needed with IAM roles) |
| `AWS_SECRET_ACCESS_KEY` | _(none)_ | AWS secret key (not needed with IAM roles) |
| `AWS_REGION` | `us-east-1` | AWS region |

### Embedding Provider

| Variable | Default | Description |
|----------|---------|-------------|
| `OMEM_EMBED_PROVIDER` | `noop` | Provider: `noop`, `bedrock`, `openai-compatible` |
| `OMEM_EMBED_API_KEY` | _(none)_ | API key (for openai-compatible) |
| `OMEM_EMBED_BASE_URL` | _(none)_ | Base URL (for openai-compatible) |
| `OMEM_EMBED_MODEL` | _(none)_ | Model name (for openai-compatible) |

**Provider details:**

| Provider | Model | Dimensions | Notes |
|----------|-------|------------|-------|
| `noop` | — | 1024 (zeros) | For testing only, no real embeddings |
| `bedrock` | Titan Embed v2 | 1024 | Uses AWS IAM credentials |
| `openai-compatible` | Configurable | 1024 | Works with OpenAI, Jina, Voyage, etc. |

### LLM Provider

| Variable | Default | Description |
|----------|---------|-------------|
| `OMEM_LLM_PROVIDER` | _(empty)_ | Provider: `openai-compatible`, `bedrock` |
| `OMEM_LLM_API_KEY` | _(none)_ | API key (for openai-compatible) |
| `OMEM_LLM_BASE_URL` | `https://api.openai.com` | Base URL |
| `OMEM_LLM_MODEL` | `gpt-4o-mini` | Model name |

> **Note**: Without an LLM provider, `smart` mode ingestion falls back to `raw` mode (no fact extraction or reconciliation).

---

## 5. Monitoring & Observability

### Health Check

```bash
curl http://localhost:8080/health
# → {"status":"ok"}
```

The `/health` endpoint is used by:
- Docker HEALTHCHECK
- ECS health checks
- ALB target group health checks

### Structured Logging

omem outputs structured JSON logs via `tracing`:

```json
{
  "timestamp": "2025-01-15T10:30:00.123Z",
  "level": "INFO",
  "target": "omem_server::api::middleware::logging",
  "message": "request completed",
  "method": "GET",
  "path": "/v1/memories/search",
  "status": 200,
  "duration_ms": 45
}
```

### Log Levels

| Level | Use |
|-------|-----|
| `error` | Failures that need attention |
| `warn` | Degraded behavior (e.g., embedding fallback) |
| `info` | Request/response logging, lifecycle events |
| `debug` | Pipeline stage details, query plans |
| `trace` | Full request/response bodies, vector operations |

### CloudWatch Integration

When running on ECS, logs are automatically sent to CloudWatch via the `awslogs` driver. Create useful metric filters:

```bash
# Error rate
aws logs put-metric-filter \
  --log-group-name /ecs/omem-server \
  --filter-name ErrorCount \
  --filter-pattern '{ $.level = "ERROR" }' \
  --metric-transformations metricName=ErrorCount,metricNamespace=omem,metricValue=1

# Request latency (p99)
aws logs put-metric-filter \
  --log-group-name /ecs/omem-server \
  --filter-name RequestLatency \
  --filter-pattern '{ $.duration_ms = * }' \
  --metric-transformations metricName=RequestLatency,metricNamespace=omem,metricValue='$.duration_ms'
```

### Key Metrics to Monitor

| Metric | Source | Alert Threshold |
|--------|--------|-----------------|
| Health check failures | ALB target group | Any unhealthy |
| Error rate | CloudWatch logs | >1% of requests |
| Request latency (p99) | CloudWatch logs | >500ms |
| Memory count growth | `GET /v1/memories?limit=1` | Unexpected spikes |
| S3 storage size | S3 bucket metrics | Budget threshold |
| CPU/Memory utilization | ECS metrics | >80% sustained |
