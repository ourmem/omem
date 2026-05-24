# TweetClaw OpenClaw Memory Workflow

Use this workflow when an OpenClaw agent needs current X/Twitter signals and durable memory. TweetClaw handles the live X/Twitter automation path. ourmem stores the durable decisions, source URLs, selected tweet IDs, research summaries, and team handoff notes.

This keeps live data access and long-term memory separate:

| Layer | Responsibility | Keep out |
|-------|----------------|----------|
| TweetClaw | Search tweets, search tweet replies, export followers, look up users, monitor tweets, receive webhooks, upload or download media, post reviewed tweets or replies, and run giveaway draws | ourmem API keys |
| ourmem | Store concise findings, decisions, campaign context, query notes, review outcomes, and reusable prompts | Xquik API keys, signing keys, cookies, raw direct-message bodies, and raw follower exports |

## Install Both Plugins

```bash
openclaw plugins install @ourmem/ourmem
openclaw plugins install @xquik/tweetclaw
```

TweetClaw installs from npm as `@xquik/tweetclaw`. The [TweetClaw GitHub repo](https://github.com/Xquik-dev/tweetclaw) documents the current OpenClaw setup path. Its [ClawHub page](https://clawhub.ai/plugins/@xquik/tweetclaw) is useful for discovery, but the npm package is the install source.

## Configure OpenClaw

Store credentials in OpenClaw plugin config or environment variables. Do not paste credentials into chat prompts, memory records, screenshots, issue text, or logs.

```bash
openclaw config set plugins.entries.ourmem.config.apiUrl "https://api.ourmem.ai"
openclaw config set plugins.entries.ourmem.config.apiKey "$OMEM_API_KEY"
openclaw config set plugins.entries.tweetclaw.config.apiKey "$XQUIK_API_KEY"
openclaw config set tools.alsoAllow '["explore", "tweetclaw", "memory_store", "memory_search"]'
```

Use TweetClaw's `explore` tool before live calls. It searches the local endpoint catalog and does not require credentials. Enable the live `tweetclaw` tool only for sessions where X/Twitter actions are expected.

## Verify Runtime Loading

```bash
openclaw plugins list
openclaw plugins inspect tweetclaw --runtime
```

Expected result:

- `@ourmem/ourmem` is installed and exposes memory tools.
- TweetClaw loads with `explore` and optional `tweetclaw`.
- `tools.alsoAllow` includes the tools the agent should be able to call.
- TweetClaw live calls return setup guidance until an API key or supported payment mode is configured.

## Suggested Agent Flow

Ask the agent to keep the source data temporary and store only durable conclusions:

```text
Use TweetClaw to search tweets about "OpenClaw memory plugins" from the last 7 days.
Summarize only public, reusable findings.
Store a memory with the query, selected tweet URLs or IDs, useful patterns, and follow-up decision.
Do not store API keys, cookies, direct-message bodies, or raw follower exports.
```

Useful TweetClaw jobs for memory-backed workflows:

| TweetClaw job | Store in ourmem |
|---------------|-----------------|
| Search tweets or search tweet replies | Query, date range, selected URLs, short evidence summary, and follow-up decision |
| Follower export | Segment summary, count, public profile URLs worth revisiting, and exclusion criteria |
| User lookup | Public profile facts, relevance notes, and last checked date |
| Monitor tweets or webhooks | Alert policy, event type, selected event URLs, and decision history |
| Media upload or media download | Public media URL, usage note, rights or review decision, and related campaign |
| Post tweets or post tweet replies | Approved copy, reviewer, posted URL, and reason for the reply |
| Giveaway draws | Eligibility rules, draw URL, winner evidence, and audit note |

## Memory Shape

Keep stored memories compact and auditable:

```json
{
  "content": "TweetClaw search for OpenClaw memory plugins on 2026-05-23 found 3 useful public examples. Keep tracking repos that pair OpenClaw plugins with persistent memory and prefer docs PRs with install verification.",
  "tags": ["tweetclaw", "openclaw", "x-twitter", "research"],
  "source": "openclaw"
}
```

Avoid storing:

- API keys, signing keys, cookies, OAuth tokens, passwords, or session material.
- Raw direct-message text.
- Full follower exports.
- Payment material.
- Private account identifiers unless the owner approved storing them.

## Team Handoff

When a finding matters beyond one agent, store the memory and share it to a team space:

```text
Store the TweetClaw research summary in ourmem with tags tweetclaw, openclaw, research.
Share it with the social-automation team space if it contains a reusable repo candidate or outreach decision.
```

This lets coding, marketing, and research agents recall the same public X/Twitter evidence without rerunning the same searches or copying credentials between tools.
