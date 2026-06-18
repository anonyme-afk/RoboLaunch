// MCP stdio ↔ host TCP bridge for Roblox Studio's MCP server.
//
// Roblox Studio's MCP launcher (`%LOCALAPPDATA%\Roblox\mcp.bat`) is
// Windows-only and unreachable from inside the Linux guest. This
// script stands in as the MCP server binary from the agent's point
// of view: it reads JSON-RPC messages on its stdin and exchanges
// them with the host via TCP. The host-side listener (see
// `src-tauri/src/mcp/roblox_proxy.rs`) spawns `cmd.exe /c mcp.bat`
// per incoming connection.
//
// Lazy mode: instead of opening the TCP socket (and triggering the
// host-side mcp.bat spawn) at script start, we respond to
// `initialize` / `notifications/initialized` / `tools/list` locally
// when possible, and only connect upstream on the first message
// that actually requires it (typically `tools/call`). Tool list is
// cached to disk after a successful upstream init so subsequent
// launches answer `tools/list` from cache in milliseconds. The very
// first launch on a fresh install still pays the cold cost; every
// launch after that skips it.
//
// Target host/port/secret are injected as env vars by the Rust
// config writer; `10.0.2.2` is gvproxy's host-loopback alias. The
// secret is sent as the first newline-terminated line
// on the socket so an unauthorised local process that guessed the
// port (Windows loopback has no per-UID isolation) cannot drive
// mcp.bat.

const net = require('net');
const readline = require('readline');
const fs = require('fs');
const path = require('path');

const host = process.env.ROBOLAUNCH_ROBLOX_MCP_HOST || '10.0.2.2';
const port = parseInt(process.env.ROBOLAUNCH_ROBLOX_MCP_PORT || '0', 10);
const secret = process.env.ROBOLAUNCH_ROBLOX_MCP_SECRET || '';

if (!port) {
  process.stderr.write('roblox-mcp-proxy: ROBOLAUNCH_ROBLOX_MCP_PORT not set\n');
  process.exit(1);
}

if (!secret) {
  process.stderr.write('roblox-mcp-proxy: ROBOLAUNCH_ROBLOX_MCP_SECRET not set\n');
  process.exit(1);
}

const CACHE_DIR = path.join(process.env.HOME || '/tmp', '.robolaunch');
const CACHE_FILE = path.join(CACHE_DIR, 'roblox-mcp-tools-cache.json');
const CACHE_VERSION = 4;
const CACHE_TTL_MS = 24 * 60 * 60 * 1000; // 1 day

// Keep the policy narrow: block script source read/search/edit helpers and
// material generation, but leave Studio's map-building asset flow available
// (`insert_from_creator_store` first, `generate_mesh` as a fallback).
// Everything else exposed by Roblox Studio's MCP server is allowed through.
const DENIED_TOOLS = new Set([
  'script_read',
  'multi_edit',
  'script_search',
  'script_grep',
  'generate_material',
]);

function rpcIdKey(id) {
  return JSON.stringify(id);
}

function toolName(tool) {
  return tool && typeof tool.name === 'string' ? tool.name : '';
}

function isDeniedToolName(name) {
  return typeof name === 'string' && DENIED_TOOLS.has(name);
}

function filterDeniedTools(tools) {
  return tools.filter((tool) => !isDeniedToolName(toolName(tool)));
}

let cachedTools = null;
try {
  const parsed = JSON.parse(fs.readFileSync(CACHE_FILE, 'utf8'));
  if (
    parsed.version === CACHE_VERSION &&
    Array.isArray(parsed.tools) &&
    Date.now() - (parsed.updatedAt || 0) <= CACHE_TTL_MS
  ) {
    cachedTools = filterDeniedTools(parsed.tools);
  }
} catch {}

function saveTools(tools) {
  try {
    fs.mkdirSync(CACHE_DIR, { recursive: true });
    const tmp = CACHE_FILE + '.tmp';
    const visibleTools = filterDeniedTools(tools);
    fs.writeFileSync(
      tmp,
      JSON.stringify({ version: CACHE_VERSION, tools: visibleTools, updatedAt: Date.now() }),
      'utf8',
    );
    fs.renameSync(tmp, CACHE_FILE);
    cachedTools = visibleTools;
  } catch (e) {
    process.stderr.write(`roblox-mcp-proxy: cache write: ${e.message}\n`);
  }
}

function send(obj) {
  process.stdout.write(JSON.stringify(obj) + '\n');
}

function sendToolBlocked(id, name) {
  send({
    jsonrpc: '2.0',
    id,
    result: {
      content: [{
        type: 'text',
        text: `Roblox Studio tool "${name || '<missing>'}" is blocked by RoboLaunch policy.`,
      }],
      isError: true,
    },
  });
}

let sock = null;
let upstreamReadyP = null;
const pendingToolsListIds = new Set();

// Connect to the host bridge and complete an init handshake on behalf
// of the agent. The agent has already received our local stub init
// response (see the rl handler below), so the upstream init uses
// private ids ("__init__" / "__tools__") that are filtered out before
// any line is forwarded to stdout. After init succeeds the socket
// becomes a 1:1 pass-through for everything else.
function connectUpstream() {
  if (upstreamReadyP) return upstreamReadyP;
  upstreamReadyP = new Promise((resolve, reject) => {
    const s = net.createConnection({ host, port });
    let resolved = false;

    s.once('connect', () => {
      sock = s;
      // Auth must precede any agent bytes on the wire.
      s.write(secret + '\n');
      s.write(JSON.stringify({
        jsonrpc: '2.0', id: '__init__',
        method: 'initialize',
        params: {
          protocolVersion: '2024-11-05',
          capabilities: {},
          clientInfo: { name: 'robolaunch-roblox-proxy', version: '1.0.0' },
        },
      }) + '\n');

      let buf = '';
      let initSeen = false;
      let toolsRefreshSeen = false;

      s.on('data', (chunk) => {
        buf += chunk.toString();
        let nl;
        while ((nl = buf.indexOf('\n')) !== -1) {
          const line = buf.slice(0, nl);
          buf = buf.slice(nl + 1);
          if (!line) continue;

          // Filter our private init dance out of the agent-facing stream.
          if (!initSeen || !toolsRefreshSeen) {
            try {
              const m = JSON.parse(line);
              if (!initSeen && m.id === '__init__') {
                initSeen = true;
                // Notifications/initialized + a tools/list refresh in the
                // same write batch so the cache stays current. Both are
                // fire-and-forget from the upstream's perspective; we just
                // filter the responses below.
                s.write(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }) + '\n');
                s.write(JSON.stringify({ jsonrpc: '2.0', id: '__tools__', method: 'tools/list' }) + '\n');
                if (!resolved) { resolved = true; resolve(); }
                continue;
              }
              if (initSeen && !toolsRefreshSeen && m.id === '__tools__') {
                toolsRefreshSeen = true;
                if (m.result && Array.isArray(m.result.tools)) {
                  saveTools(m.result.tools);
                }
                continue;
              }
            } catch {}
          }

          try {
            const m = JSON.parse(line);
            if (m.id !== undefined && m.id !== null) {
              const key = rpcIdKey(m.id);
              if (pendingToolsListIds.has(key)) {
                pendingToolsListIds.delete(key);
                if (m.result && Array.isArray(m.result.tools)) {
                  m.result = {
                    ...m.result,
                    tools: filterDeniedTools(m.result.tools),
                  };
                }
                process.stdout.write(JSON.stringify(m) + '\n');
                continue;
              }
            }
          } catch {}
          process.stdout.write(line + '\n');
        }
      });

      s.on('close', () => {
        // Host-side mcp.bat exited or the socket was dropped. Exit
        // cleanly so the agent sees an EOF rather than a hung
        // subprocess.
        process.exit(0);
      });
      s.on('error', (err) => {
        process.stderr.write(`roblox-mcp-proxy: socket: ${err.message}\n`);
        process.exit(1);
      });
    });

    s.on('error', (err) => {
      // Connect-time error. Don't exit yet — let the rl handler turn
      // this into a JSON-RPC error response for the in-flight message.
      if (!resolved) { resolved = true; reject(err); }
    });
  });
  return upstreamReadyP;
}

const rl = readline.createInterface({ input: process.stdin, terminal: false });

rl.on('line', async (line) => {
  let msg;
  try { msg = JSON.parse(line); } catch {
    // Non-JSON garbage. If upstream is connected, pass it through (it
    // will reject); otherwise drop. Matches the original pipe's
    // permissive behaviour.
    if (sock) sock.write(line + '\n');
    return;
  }

  if (msg.method === 'initialize') {
    // Local stub. Capabilities mirror the conservative subset the
    // host bridge has been observed to expose: tools-only. If a
    // future Roblox MCP version starts advertising prompts /
    // resources / logging, refresh this list and the agent will
    // pick it up on next launch.
    send({
      jsonrpc: '2.0', id: msg.id,
      result: {
        protocolVersion: '2024-11-05',
        capabilities: { tools: {} },
        serverInfo: { name: 'robolaunch-roblox-proxy', version: '1.0.0' },
      },
    });
    return;
  }
  if (msg.method === 'notifications/initialized') {
    // No-op locally. We send our own to upstream after we connect.
    return;
  }
  if (msg.method === 'tools/list' && cachedTools) {
    send({ jsonrpc: '2.0', id: msg.id, result: { tools: cachedTools } });
    // Background refresh — if the connection eventually completes the
    // cache will be updated for next launch. Failures are swallowed;
    // the next launch will retry.
    connectUpstream().catch(() => {});
    return;
  }

  if (msg.method === 'tools/list' && msg.id !== undefined && msg.id !== null) {
    pendingToolsListIds.add(rpcIdKey(msg.id));
  }

  if (msg.method === 'tools/call') {
    const name = msg.params && typeof msg.params.name === 'string' ? msg.params.name : '';
    if (isDeniedToolName(name)) {
      sendToolBlocked(msg.id, name);
      return;
    }
  }

  // Cold tools/list (no cache) and every tools/call goes upstream.
  try {
    await connectUpstream();
  } catch (err) {
    if (msg.method === 'tools/list' && msg.id !== undefined && msg.id !== null) {
      pendingToolsListIds.delete(rpcIdKey(msg.id));
    }
    if (msg.id !== undefined && msg.id !== null) {
      send({
        jsonrpc: '2.0', id: msg.id,
        error: { code: -32603, message: 'roblox-mcp-proxy upstream connect failed: ' + err.message },
      });
    }
    return;
  }
  sock.write(line + '\n');
});

rl.on('close', () => {
  if (sock) sock.end();
  else process.exit(0);
});
