#!/usr/bin/env node
// Lightweight per-agent MCP stdio stub.
//
// Agent CLIs still require a command that speaks MCP over stdio. The real
// broker now lives in the host Tauri process; this guest-side adapter only
// forwards JSON-RPC over local HTTP through gvproxy and reads guest-local
// files for upload_file.

const crypto = require('crypto');
const fs = require('fs');
const http = require('http');
const path = require('path');
const readline = require('readline');

const GATEWAY_HOST = process.env.ROBOLAUNCH_GATEWAY_HOST || '10.0.2.2';
const GATEWAY_PORT = Number(process.env.ROBOLAUNCH_GATEWAY_PORT || 0);
const GATEWAY_TOKEN = process.env.ROBOLAUNCH_AGENT_MCP_TOKEN || '';
const CLIENT_ID = process.env.ROBOLAUNCH_MCP_CLIENT_ID || crypto.randomUUID();
const RPC_TIMEOUT_MS = 11 * 60 * 1000;

let AGENT_ID = process.env.ROBOLAUNCH_AGENT_ID || '';
let AGENT_NUM = process.env.ROBOLAUNCH_AGENT_NUM || '0';
let AGENT_NAME = process.env.ROBOLAUNCH_AGENT_NAME || '';
let TAB_LABEL = process.env.ROBOLAUNCH_TAB_LABEL || 'Unknown';
let AGENT_TYPE = process.env.ROBOLAUNCH_AGENT_TYPE || 'unknown';
let closeSent = false;

function log(...args) {
  process.stderr.write('[robolaunch-mcp-stub] ' + args.join(' ') + '\n');
}

function readProcessEnv(pid) {
  try {
    const buf = fs.readFileSync(`/proc/${pid}/environ`);
    const out = {};
    for (const entry of buf.toString('utf8').split('\0')) {
      const eq = entry.indexOf('=');
      if (eq > 0) out[entry.slice(0, eq)] = entry.slice(eq + 1);
    }
    return out;
  } catch {
    return null;
  }
}

function readProcessPpid(pid) {
  try {
    const status = fs.readFileSync(`/proc/${pid}/status`, 'utf8');
    const m = status.match(/^PPid:\s+(\d+)/m);
    return m ? parseInt(m[1], 10) : null;
  } catch {
    return null;
  }
}

function inheritIdentityFromAncestors() {
  if (AGENT_ID && AGENT_TYPE && AGENT_TYPE !== 'unknown') return;
  let pid = process.ppid;
  for (let i = 0; i < 8 && pid && pid !== 1; i++) {
    const env = readProcessEnv(pid);
    if (env && env.ROBOLAUNCH_AGENT_ID) {
      AGENT_ID = env.ROBOLAUNCH_AGENT_ID;
      AGENT_NUM = env.ROBOLAUNCH_AGENT_NUM || AGENT_NUM;
      AGENT_NAME = env.ROBOLAUNCH_AGENT_NAME || AGENT_NAME;
      TAB_LABEL = env.ROBOLAUNCH_TAB_LABEL || TAB_LABEL;
      AGENT_TYPE = env.ROBOLAUNCH_AGENT_TYPE || AGENT_TYPE;
      return;
    }
    const ppid = readProcessPpid(pid);
    if (!ppid || ppid === pid) break;
    pid = ppid;
  }
}

function gatewayHeaders(extra = {}) {
  return {
    Authorization: `Bearer ${GATEWAY_TOKEN}`,
    'X-RoboLaunch-Client-Id': CLIENT_ID,
    'X-RoboLaunch-Agent-Id': AGENT_ID,
    'X-RoboLaunch-Agent-Num': AGENT_NUM,
    'X-RoboLaunch-Agent-Name': AGENT_NAME,
    'X-RoboLaunch-Tab-Label': TAB_LABEL,
    'X-RoboLaunch-Agent-Type': AGENT_TYPE,
    ...extra,
  };
}

function requestGateway(method, endpoint, body, headers = {}) {
  return new Promise((resolve, reject) => {
    if (!GATEWAY_PORT || !GATEWAY_TOKEN) {
      reject(new Error('MCP gateway env is missing; the RoboLaunch app must be running.'));
      return;
    }

    const payload = body === undefined || body === null
      ? null
      : Buffer.isBuffer(body)
        ? body
        : Buffer.from(typeof body === 'string' ? body : JSON.stringify(body));

    const req = http.request({
      hostname: GATEWAY_HOST,
      port: GATEWAY_PORT,
      path: endpoint,
      method,
      headers: gatewayHeaders({
        ...(payload ? { 'Content-Length': payload.length } : {}),
        ...headers,
      }),
    }, (res) => {
      const chunks = [];
      res.on('data', (chunk) => chunks.push(chunk));
      res.on('end', () => {
        const text = Buffer.concat(chunks).toString('utf8');
        if (res.statusCode === 204) {
          resolve(null);
          return;
        }
        if (res.statusCode < 200 || res.statusCode >= 300) {
          reject(new Error(text || `HTTP ${res.statusCode}`));
          return;
        }
        if (!text.trim()) {
          resolve(null);
          return;
        }
        try {
          resolve(JSON.parse(text));
        } catch (e) {
          reject(new Error('Invalid gateway JSON: ' + e.message));
        }
      });
    });

    req.setTimeout(RPC_TIMEOUT_MS, () => req.destroy(new Error('MCP gateway request timed out')));
    req.on('error', reject);
    if (payload) req.write(payload);
    req.end();
  });
}

function uploadFileToGateway(filePath) {
  return new Promise((resolve, reject) => {
    if (!GATEWAY_PORT || !GATEWAY_TOKEN) {
      reject(new Error('MCP gateway env is missing; the RoboLaunch app must be running.'));
      return;
    }
    fs.stat(filePath, (statErr, stat) => {
      if (statErr) {
        reject(statErr);
        return;
      }
      if (!stat.isFile()) {
        reject(new Error('Not a regular file: ' + filePath));
        return;
      }

      const req = http.request({
        hostname: GATEWAY_HOST,
        port: GATEWAY_PORT,
        path: '/mcp/upload-file',
        method: 'POST',
        headers: gatewayHeaders({
          'Content-Type': 'application/octet-stream',
          'Content-Length': stat.size,
          'X-File-Name': path.basename(filePath),
        }),
      }, (res) => {
        const chunks = [];
        res.on('data', (chunk) => chunks.push(chunk));
        res.on('end', () => {
          const text = Buffer.concat(chunks).toString('utf8');
          if (res.statusCode < 200 || res.statusCode >= 300) {
            reject(new Error(text || `HTTP ${res.statusCode}`));
            return;
          }
          try {
            resolve(JSON.parse(text));
          } catch (e) {
            reject(new Error('Invalid upload response JSON: ' + e.message));
          }
        });
      });
      req.setTimeout(RPC_TIMEOUT_MS, () => req.destroy(new Error('MCP upload timed out')));
      req.on('error', reject);
      fs.createReadStream(filePath).on('error', reject).pipe(req);
    });
  });
}

async function waitForHealth() {
  for (let i = 0; i < 60; i++) {
    try {
      await requestGateway('GET', '/mcp/health');
      return;
    } catch (e) {
      if (i === 0 || i === 20 || i === 40) {
        log('waiting for host MCP gateway:', e.message || String(e));
      }
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
  }
  throw new Error('host MCP gateway did not become ready');
}

function writeRpc(msg) {
  process.stdout.write(JSON.stringify(msg) + '\n');
}

function toolTextResult(id, text) {
  return {
    jsonrpc: '2.0',
    id,
    result: { content: [{ type: 'text', text }] },
  };
}

function toolErrorResult(id, text) {
  return {
    jsonrpc: '2.0',
    id,
    result: { content: [{ type: 'text', text }], isError: true },
  };
}

function rpcError(id, message) {
  return {
    jsonrpc: '2.0',
    id,
    error: { code: -32000, message },
  };
}

async function handleUploadFile(msg) {
  const filePath = msg && msg.params && msg.params.arguments
    ? msg.params.arguments.file_path
    : '';
  if (typeof filePath !== 'string' || !filePath.trim()) {
    writeRpc(toolErrorResult(msg.id, 'Upload failed: file_path is required.'));
    return;
  }
  try {
    const result = await uploadFileToGateway(filePath);
    const text = JSON.stringify({ serverFileId: result.serverFileId });
    writeRpc(toolTextResult(msg.id, text));
  } catch (e) {
    writeRpc(toolErrorResult(msg.id, 'Upload failed: ' + (e.message || String(e))));
  }
}

async function handleLine(line) {
  let msg;
  try {
    msg = JSON.parse(line);
  } catch {
    return;
  }

  if (msg.method === 'tools/call' && msg.params && msg.params.name === 'upload_file') {
    await handleUploadFile(msg);
    return;
  }

  try {
    const response = await requestGateway('POST', '/mcp/rpc', msg, {
      'Content-Type': 'application/json',
    });
    if (response) writeRpc(response);
  } catch (e) {
    log('rpc failed:', e.message || String(e));
    if (msg.id !== undefined && msg.id !== null) {
      writeRpc(rpcError(msg.id, e.message || String(e)));
    }
  }
}

function closeGateway() {
  if (closeSent) return Promise.resolve();
  closeSent = true;
  return requestGateway('POST', '/mcp/close').catch(() => {});
}

(async () => {
  inheritIdentityFromAncestors();

  try {
    await waitForHealth();
  } catch (e) {
    log('connect failed:', e.message || String(e));
    process.exit(1);
  }

  const rl = readline.createInterface({ input: process.stdin, terminal: false });
  rl.on('line', (line) => {
    handleLine(line).catch((e) => log('line handler failed:', e.message || String(e)));
  });
  rl.on('close', async () => {
    await closeGateway();
  });

  for (const signal of ['SIGINT', 'SIGTERM']) {
    process.on(signal, () => {
      closeGateway().finally(() => process.exit(0));
    });
  }
})();
