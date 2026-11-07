import './style.css';

const DEFAULT_RELAY_CANDIDATES = [
  '/dns4/auto-relay.libp2p.io/tcp/443/wss/p2p/QmWDn2LY8nannvSWJzruUYoLZ4vV83vfCBwd8DipvdgQc3',
  '/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN',
  '/dnsaddr/bootstrap.libp2p.io/p2p/QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa',
  '/dnsaddr/bootstrap.libp2p.io/p2p/QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb',
  '/dnsaddr/bootstrap.libp2p.io/p2p/QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt',
  '/dnsaddr/va1.bootstrap.libp2p.io/p2p/12D3KooWKnDdG3iXw9eTFijk3EWSunZcFi54Zka4wmtqtt6rPxc8'
];
const MAX_LOG_LINES = 400;
const encoder = new TextEncoder();
const decoder = new TextDecoder();
const runs = new Map();
let peer;
let topicName;
let localPeerId;

const $ = (selector) => document.querySelector(selector);
const examplePicker = $('#example');
const runButton = $('#run');
const runsView = $('#runs');
const networkLog = $('#network-log');
const status = $('#network-status');
const connectButton = $('#connect');
const dialPeerButton = $('#dial-peer');
const disconnectButton = $('#disconnect');
const copyAddressButton = $('#copy-address');
const chatForm = $('#chat-form');
const chatMessage = $('#chat-message');

$('#relay-list').value = localStorage.getItem('byz-relays-v3') || DEFAULT_RELAY_CANDIDATES.join('\n');

function appendLog(target, text) {
  const lines = [...target.textContent.split('\n'), text].filter(Boolean).slice(-MAX_LOG_LINES);
  target.textContent = lines.join('\n');
  target.scrollTop = target.scrollHeight;
}

function describeError(error) {
  if (error instanceof Error && error.message) return error.message;
  if (error && typeof error === 'object' && 'message' in error && error.message) {
    return String(error.message);
  }
  return String(error);
}

function setNetworkState(label, state) {
  status.textContent = label;
  status.dataset.state = state;
}

function updateLocalAddresses() {
  if (!peer) return;
  const addresses = peer.getMultiaddrs().map((address) => address.toString());
  const shareableAddress = addresses.find((address) => address.includes('/p2p-circuit') && address.includes('/webrtc'))
    || addresses.find((address) => address.includes('/p2p-circuit') && address.includes('/wss'))
    || addresses.find((address) => address.includes('/p2p-circuit'))
    || addresses.find((address) => address.includes('/webrtc'))
    || addresses[0];
  $('#local-addresses').value = shareableAddress || '';
  copyAddressButton.disabled = !shareableAddress;
}

function addNetworkLog(message) {
  const line = `[${new Date().toLocaleTimeString()}] ${message}`;
  appendLog(networkLog, line);
}

function createRunCard(example) {
  const card = document.createElement('article');
  card.className = 'run';
  const header = document.createElement('div');
  header.className = 'run-header';
  const title = document.createElement('span');
  title.className = 'run-title';
  title.textContent = `${example} · ${new Date().toLocaleTimeString()}`;
  const state = document.createElement('span');
  state.className = 'run-state';
  state.setAttribute('role', 'status');
  state.textContent = 'Starting';
  const stop = document.createElement('button');
  stop.type = 'button';
  stop.textContent = 'Stop';
  header.append(title, state, stop);
  const log = document.createElement('pre');
  log.className = 'log';
  log.setAttribute('role', 'log');
  log.setAttribute('aria-label', `${example} output`);
  log.setAttribute('aria-live', 'off');
  card.append(header, log);
  runsView.prepend(card);
  return { card, log, state, stop };
}

function broadcastLog(example, line) {
  if (!$('#share-logs').checked || !peer || !topicName) return;
  const data = encoder.encode(JSON.stringify({
    type: 'example-log',
    example,
    line,
    peer: localPeerId,
    time: Date.now()
  }));
  peer.services.pubsub.publish(topicName, data).catch((error) => {
    addNetworkLog(`Could not publish example log: ${describeError(error)}`);
  });
}

function runSimulation(example) {
  const id = crypto.randomUUID();
  const card = createRunCard(example);
  const worker = new Worker(new URL('./simulation-worker.js', import.meta.url), { type: 'module' });
  const run = { worker, ...card };
  runs.set(id, run);
  appendLog(run.log, `Launching ${example} in an isolated WASM worker.`);
  worker.addEventListener('message', ({ data }) => {
    if (data.type === 'log') {
      appendLog(run.log, data.line);
      broadcastLog(example, data.line);
    } else if (data.type === 'complete') {
      run.state.textContent = 'Complete';
      run.stop.disabled = true;
      appendLog(run.log, 'Example completed.');
    } else if (data.type === 'error') {
      run.state.textContent = 'Failed';
      run.state.dataset.state = 'error';
      run.stop.disabled = true;
      appendLog(run.log, `ERROR: ${data.message}`);
    }
  });
  worker.addEventListener('error', (event) => {
    run.state.textContent = 'Failed';
    run.state.dataset.state = 'error';
    run.stop.disabled = true;
    appendLog(run.log, `Worker error: ${event.message}`);
  });
  run.stop.addEventListener('click', () => {
    worker.terminate();
    run.state.textContent = 'Stopped';
    run.stop.disabled = true;
    appendLog(run.log, 'Run stopped by user.');
  }, { once: true });
  worker.postMessage({ type: 'run', example });
  addNetworkLog(`Started ${example} in worker ${id}.`);
}

async function connectNetwork() {
  if (peer) return;
  const { createLibp2p } = await import('libp2p');
  const { webSockets } = await import('@libp2p/websockets');
  const { webRTC } = await import('@libp2p/webrtc');
  const { circuitRelayTransport } = await import('@libp2p/circuit-relay-v2');
  const { bootstrap } = await import('@libp2p/bootstrap');
  const { gossipsub } = await import('@chainsafe/libp2p-gossipsub');
  const { noise } = await import('@chainsafe/libp2p-noise');
  const { yamux } = await import('@chainsafe/libp2p-yamux');
  const { identify } = await import('@libp2p/identify');
  const { dcutr } = await import('@libp2p/dcutr');
  const { multiaddr } = await import('@multiformats/multiaddr');

  topicName = $('#topic').value.trim();
  if (!topicName) throw new Error('Enter a gossip topic before connecting.');
  const relays = $('#relay-list').value.split('\n').map((address) => address.trim()).filter(Boolean);
  if (relays.length === 0) throw new Error('Enter at least one relay candidate.');
  localStorage.setItem('byz-relays-v3', relays.join('\n'));

  setNetworkState('Starting…', '');
  addNetworkLog(`Starting browser peer for topic "${topicName}".`);
  peer = await createLibp2p({
    addresses: { listen: ['/p2p-circuit', '/webrtc'] },
    transports: [
      webSockets(),
      webRTC({
        rtcConfiguration: {
          iceServers: [
            { urls: 'stun:stun.l.google.com:19302' },
            { urls: 'stun:global.stun.twilio.com:3478' },
            { urls: 'stun:stun.cloudflare.com:3478' },
            { urls: 'stun:stun.services.mozilla.com:3478' }
          ]
        }
      }),
      circuitRelayTransport()
    ],
    connectionEncrypters: [noise()],
    streamMuxers: [yamux()],
    peerDiscovery: [bootstrap({ list: relays })],
    connectionGater: { denyDialMultiaddr: () => false },
    services: {
      identify: identify(),
      dcutr: dcutr(),
      pubsub: gossipsub({ allowPublishToZeroTopicPeers: true })
    }
  });
  localPeerId = peer.peerId.toString();
  peer.addEventListener('peer:connect', (event) => {
    addNetworkLog(`Connected to peer ${event.detail.remotePeer}.`);
    updateLocalAddresses();
    setNetworkState(`Connected · ${peer.getConnections().length} peer(s)`, 'connected');
  });
  peer.addEventListener('peer:disconnect', (event) => {
    addNetworkLog(`Disconnected from peer ${event.detail.remotePeer}.`);
    setNetworkState(`Connected · ${peer.getConnections().length} peer(s)`, 'connected');
  });
  peer.addEventListener('self:peer:update', updateLocalAddresses);
  peer.services.pubsub.addEventListener('message', ({ detail }) => {
    if (detail.topic !== topicName) return;
    const source = detail.from.toString();
    if (source === localPeerId) return;
    const payload = decoder.decode(detail.data);
    try {
      const message = JSON.parse(payload);
      if (message.type === 'example-log') {
        addNetworkLog(`${message.example} from ${source}: ${message.line}`);
      } else if (message.type === 'chat') {
        addNetworkLog(`${source}: ${message.message}`);
      } else {
        addNetworkLog(`${source}: ${payload}`);
      }
    } catch {
      addNetworkLog(`${source}: ${payload}`);
    }
  });

  await peer.start();
  updateLocalAddresses();
  peer.services.pubsub.subscribe(topicName);
  addNetworkLog(`Peer ID: ${localPeerId}`);

  const attempts = relays.map(async (address) => {
    try {
      addNetworkLog(`Trying relay candidate ${address}`);
      await peer.dial(multiaddr(address), { signal: AbortSignal.timeout(10000) });
      updateLocalAddresses();
      addNetworkLog(`Connected to candidate ${address}; waiting for relay reservation and peer discovery.`);
      return true;
    } catch (error) {
      addNetworkLog(`Relay candidate unavailable (${address}): ${describeError(error)}`);
      return false;
    }
  });
  const successfulDials = (await Promise.all(attempts)).filter(Boolean).length;
  if (successfulDials === 0) {
    addNetworkLog('No relay candidate accepted a connection. Local examples still run; remote gossip is unavailable.');
    setNetworkState('No relay connected', 'error');
  } else {
    setNetworkState(`Connected · ${peer.getConnections().length} peer(s)`, 'connected');
  }
  connectButton.disabled = true;
  dialPeerButton.disabled = false;
  disconnectButton.disabled = false;
  chatForm.querySelector('button').disabled = false;
}

async function dialPeer() {
  if (!peer) throw new Error('Connect to the network before dialing a peer.');
  const { multiaddr } = await import('@multiformats/multiaddr');
  const address = $('#peer-address').value.trim();
  if (!address) throw new Error('Enter the peer multiaddress to dial.');
  addNetworkLog(`Dialing peer address ${address}`);
  await peer.dial(multiaddr(address), { signal: AbortSignal.timeout(10000) });
  addNetworkLog(`Peer address connected: ${address}`);
}

async function disconnectNetwork() {
  if (!peer) return;
  const currentPeer = peer;
  peer = undefined;
  topicName = undefined;
  localPeerId = undefined;
  try {
    await currentPeer.stop();
  } finally {
    connectButton.disabled = false;
    $('#local-addresses').value = '';
    copyAddressButton.disabled = true;
    dialPeerButton.disabled = true;
    disconnectButton.disabled = true;
    chatForm.querySelector('button').disabled = true;
    setNetworkState('Not connected', '');
    addNetworkLog('Disconnected from the gossip network.');
  }
}

runButton.addEventListener('click', () => {
  const example = examplePicker.value;
  if (example === 'p2p_node') {
    connectNetwork().catch((error) => {
      setNetworkState('Connection failed', 'error');
      addNetworkLog(`Could not start browser peer: ${describeError(error)}`);
    });
    return;
  }
  runSimulation(example);
});

connectButton.addEventListener('click', () => connectNetwork().catch((error) => {
  setNetworkState('Connection failed', 'error');
  addNetworkLog(`Could not start browser peer: ${describeError(error)}`);
}));
disconnectButton.addEventListener('click', () => disconnectNetwork().catch((error) => {
  addNetworkLog(`Could not stop browser peer: ${describeError(error)}`);
}));
copyAddressButton.addEventListener('click', async () => {
  try {
    await navigator.clipboard.writeText($('#local-addresses').value);
    addNetworkLog('Peer address copied to clipboard.');
  } catch (error) {
    addNetworkLog(`Could not copy peer address: ${describeError(error)}`);
  }
});
dialPeerButton.addEventListener('click', () => dialPeer().catch((error) => {
  addNetworkLog(`Could not dial peer: ${describeError(error)}`);
}));
chatForm.addEventListener('submit', async (event) => {
  event.preventDefault();
  const message = chatMessage.value.trim();
  if (!message || !peer || !topicName) return;
  try {
    await peer.services.pubsub.publish(topicName, encoder.encode(JSON.stringify({
      type: 'chat',
      message,
      peer: localPeerId,
      time: Date.now()
    })));
    addNetworkLog(`You: ${message}`);
    chatMessage.value = '';
  } catch (error) {
    addNetworkLog(`Could not publish message: ${describeError(error)}`);
  }
});

window.addEventListener('beforeunload', () => {
  for (const { worker } of runs.values()) worker.terminate();
  if (peer) void peer.stop();
});
