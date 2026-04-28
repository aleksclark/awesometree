# echo-agent

Minimal mock A2A agent for ARP compliance testing. Serves a valid AgentCard
and echoes back any message it receives.

## Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/.well-known/agent-card.json` | Returns AgentCard JSON |
| POST | `/message:send` | Returns echoed Task response |
| POST | `/message:stream` | Returns echoed SSE stream |

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `ECHO_AGENT_URL` | `http://echo-agent:9200` | URL advertised in AgentCard `supportedInterfaces` |
| `ECHO_AGENT_ADDR` | `:9200` | Listen address |

## Build & Run

```bash
# Standalone
cd agents/echo-agent
go build -o echo-agent .
./echo-agent

# Docker
docker build -t echo-agent agents/echo-agent/
docker run -p 9200:9200 echo-agent

# With ARP server (from repo root)
docker compose -f test/docker-compose.arp.yml up --build
```

## state.json Changes Required

The awesometree test fixture `state.json` needs its agent entry updated so the
ARP server can reach the echo agent via Docker DNS:

```diff
- "direct_url": "http://localhost:9200"
+ "direct_url": "http://echo-agent:9200"
```

The `port` field (9200) stays the same — only the hostname changes from
`localhost` to `echo-agent` (the Docker Compose service name).
