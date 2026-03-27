"""
omem Provider for MemoryBench

Adapter that wraps the omem REST API for use with memory benchmark frameworks.
Implements the standard MemoryProvider interface for evaluation.

Usage:
    from omem_provider import OmemProvider

    provider = OmemProvider(base_url="http://localhost:8080")
    provider.setup()
    provider.ingest(messages=[...])
    results = provider.search("query")
    provider.teardown()
"""

import json
import time
import urllib.request
import urllib.parse
import urllib.error
from dataclasses import dataclass, field
from typing import Optional


@dataclass
class Memory:
    """A single memory returned from omem."""

    id: str
    content: str
    category: str
    memory_type: str
    tier: str
    importance: float
    confidence: float
    tags: list[str] = field(default_factory=list)
    created_at: str = ""


@dataclass
class SearchResult:
    """A search result with score."""

    memory: Memory
    score: float


class OmemProvider:
    """
    MemoryBench-compatible provider for omem.

    Wraps the omem REST API to provide a standard interface for
    memory ingestion, retrieval, and evaluation.
    """

    def __init__(
        self,
        base_url: str = "http://localhost:8080",
        api_key: Optional[str] = None,
        tenant_name: str = "memorybench",
    ):
        self.base_url = base_url.rstrip("/")
        self.api_key = api_key
        self.tenant_name = tenant_name

    # ── Lifecycle ─────────────────────────────────────────────────────

    def setup(self) -> str:
        """Create a tenant and return the API key."""
        resp = self._request("POST", "/v1/tenants", {"name": self.tenant_name})
        self.api_key = resp["api_key"]
        return self.api_key

    def teardown(self) -> None:
        """Cleanup (no-op — omem doesn't expose tenant deletion)."""
        pass

    def health_check(self) -> bool:
        """Check if the omem server is healthy."""
        try:
            resp = self._request("GET", "/health")
            return resp.get("status") == "ok"
        except Exception:
            return False

    # ── Ingestion ─────────────────────────────────────────────────────

    def ingest(
        self,
        messages: list[dict],
        mode: str = "smart",
        session_id: Optional[str] = None,
    ) -> dict:
        """
        Ingest a conversation into omem.

        Args:
            messages: List of {"role": "user"|"assistant", "content": "..."}
            mode: "smart" (LLM extraction) or "raw" (store as-is)
            session_id: Optional session identifier

        Returns:
            {"task_id": "...", "stored_count": N}
        """
        body: dict = {"messages": messages, "mode": mode}
        if session_id:
            body["session_id"] = session_id
        return self._request("POST", "/v1/memories", body)

    def store(
        self,
        content: str,
        tags: Optional[list[str]] = None,
        source: Optional[str] = None,
    ) -> dict:
        """
        Directly store a single memory (pinned).

        Args:
            content: The memory content
            tags: Optional tags
            source: Optional source identifier

        Returns:
            Full Memory object
        """
        body: dict = {"content": content}
        if tags:
            body["tags"] = tags
        if source:
            body["source"] = source
        return self._request("POST", "/v1/memories", body)

    # ── Retrieval ─────────────────────────────────────────────────────

    def search(
        self,
        query: str,
        limit: int = 20,
        min_score: Optional[float] = None,
        include_trace: bool = False,
    ) -> list[SearchResult]:
        """
        Search memories by semantic query.

        Args:
            query: Natural language search query
            limit: Maximum results to return
            min_score: Minimum relevance score threshold
            include_trace: Include retrieval pipeline trace

        Returns:
            List of SearchResult objects
        """
        params = {"q": query, "limit": str(limit)}
        if min_score is not None:
            params["min_score"] = str(min_score)
        if include_trace:
            params["include_trace"] = "true"

        resp = self._request("GET", "/v1/memories/search", params=params)

        results = []
        for r in resp.get("results", []):
            mem_data = r["memory"]
            memory = Memory(
                id=mem_data["id"],
                content=mem_data["content"],
                category=mem_data["category"],
                memory_type=mem_data["memory_type"],
                tier=mem_data["tier"],
                importance=mem_data["importance"],
                confidence=mem_data["confidence"],
                tags=mem_data.get("tags", []),
                created_at=mem_data.get("created_at", ""),
            )
            results.append(SearchResult(memory=memory, score=r["score"]))

        return results

    def get(self, memory_id: str) -> Memory:
        """Get a single memory by ID."""
        data = self._request("GET", f"/v1/memories/{memory_id}")
        return Memory(
            id=data["id"],
            content=data["content"],
            category=data["category"],
            memory_type=data["memory_type"],
            tier=data["tier"],
            importance=data["importance"],
            confidence=data["confidence"],
            tags=data.get("tags", []),
            created_at=data.get("created_at", ""),
        )

    def list_memories(
        self,
        limit: int = 20,
        offset: int = 0,
    ) -> list[Memory]:
        """List memories with pagination."""
        params = {"limit": str(limit), "offset": str(offset)}
        resp = self._request("GET", "/v1/memories", params=params)

        return [
            Memory(
                id=m["id"],
                content=m["content"],
                category=m["category"],
                memory_type=m["memory_type"],
                tier=m["tier"],
                importance=m["importance"],
                confidence=m["confidence"],
                tags=m.get("tags", []),
                created_at=m.get("created_at", ""),
            )
            for m in resp.get("memories", [])
        ]

    def get_profile(self) -> dict:
        """Get the user profile."""
        return self._request("GET", "/v1/profile")

    # ── Mutation ──────────────────────────────────────────────────────

    def update(
        self,
        memory_id: str,
        content: Optional[str] = None,
        tags: Optional[list[str]] = None,
        state: Optional[str] = None,
    ) -> Memory:
        """Update a memory."""
        body: dict = {}
        if content is not None:
            body["content"] = content
        if tags is not None:
            body["tags"] = tags
        if state is not None:
            body["state"] = state

        data = self._request("PUT", f"/v1/memories/{memory_id}", body)
        return Memory(
            id=data["id"],
            content=data["content"],
            category=data["category"],
            memory_type=data["memory_type"],
            tier=data["tier"],
            importance=data["importance"],
            confidence=data["confidence"],
            tags=data.get("tags", []),
            created_at=data.get("created_at", ""),
        )

    def delete(self, memory_id: str) -> dict:
        """Soft-delete a memory."""
        return self._request("DELETE", f"/v1/memories/{memory_id}")

    # ── MemoryBench Interface ─────────────────────────────────────────

    def run_benchmark(
        self,
        dataset_path: str,
        wait_seconds: float = 2.0,
    ) -> dict:
        """
        Run a full benchmark against a dataset file.

        Args:
            dataset_path: Path to sample_conversations.json
            wait_seconds: Time to wait after ingestion for async processing

        Returns:
            {"total": N, "passed": N, "failed": N, "score": float, "details": [...]}
        """
        with open(dataset_path) as f:
            conversations = json.load(f)

        # Setup
        if not self.api_key:
            self.setup()

        # Ingest all conversations
        for conv in conversations:
            self.ingest(
                messages=conv["messages"],
                mode="raw",
                session_id=f"eval-{conv['id']}",
            )

        # Wait for async processing
        time.sleep(wait_seconds)

        # Run search queries
        total = 0
        passed = 0
        failed = 0
        details = []

        for conv in conversations:
            for sq in conv.get("search_queries", []):
                total += 1
                query = sq["query"]
                should_contain = sq.get("should_contain", "")
                should_not_contain = sq.get("should_not_contain", "")

                results = self.search(query, limit=5)
                all_content = " ".join(r.memory.content for r in results).lower()

                ok = True
                reason = ""

                if should_contain:
                    if should_contain.lower() not in all_content:
                        ok = False
                        reason = f"expected '{should_contain}' not found"

                if should_not_contain:
                    if should_not_contain.lower() in all_content:
                        ok = False
                        reason = f"unexpected '{should_not_contain}' found"

                if not results:
                    ok = False
                    reason = "no results returned"

                if ok:
                    passed += 1
                else:
                    failed += 1

                details.append(
                    {
                        "conversation": conv["id"],
                        "query": query,
                        "passed": ok,
                        "reason": reason,
                        "result_count": len(results),
                        "top_score": results[0].score if results else 0.0,
                    }
                )

        score = (passed / total * 100) if total > 0 else 0.0

        return {
            "total": total,
            "passed": passed,
            "failed": failed,
            "score": round(score, 1),
            "details": details,
        }

    # ── Internal ──────────────────────────────────────────────────────

    def _request(
        self,
        method: str,
        path: str,
        body: Optional[dict] = None,
        params: Optional[dict] = None,
    ) -> dict:
        """Make an HTTP request to the omem API."""
        url = f"{self.base_url}{path}"
        if params:
            url += "?" + urllib.parse.urlencode(params)

        headers = {"Content-Type": "application/json"}
        if self.api_key:
            headers["X-API-Key"] = self.api_key

        data = json.dumps(body).encode() if body else None

        req = urllib.request.Request(url, data=data, headers=headers, method=method)

        try:
            with urllib.request.urlopen(req, timeout=30) as resp:
                return json.loads(resp.read().decode())
        except urllib.error.HTTPError as e:
            error_body = e.read().decode() if e.fp else ""
            raise RuntimeError(
                f"omem API error: {e.code} {e.reason} — {error_body}"
            ) from e
        except urllib.error.URLError as e:
            raise RuntimeError(f"omem connection error: {e.reason}") from e


# ── CLI Entry Point ───────────────────────────────────────────────────────────

if __name__ == "__main__":
    import sys
    import os

    base_url = os.environ.get("OMEM_API_URL", "http://localhost:8080")
    dataset = os.path.join(
        os.path.dirname(__file__), "..", "datasets", "sample_conversations.json"
    )

    if len(sys.argv) > 1:
        dataset = sys.argv[1]

    provider = OmemProvider(base_url=base_url)

    if not provider.health_check():
        print(f"ERROR: omem server at {base_url} is not healthy")
        sys.exit(1)

    print(f"Running benchmark against {base_url}...")
    results = provider.run_benchmark(dataset)

    print(f"\n{'=' * 60}")
    print(f"  omem Benchmark Results")
    print(f"{'=' * 60}")
    print(f"  Total:  {results['total']}")
    print(f"  Passed: {results['passed']}")
    print(f"  Failed: {results['failed']}")
    print(f"  Score:  {results['score']}%")
    print(f"{'=' * 60}")

    if results["failed"] > 0:
        print("\n  Failures:")
        for d in results["details"]:
            if not d["passed"]:
                print(f"    - [{d['conversation']}] {d['query']}: {d['reason']}")

    sys.exit(0 if results["failed"] == 0 else 1)
