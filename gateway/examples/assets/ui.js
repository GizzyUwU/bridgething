import { WebSocketClient } from './websocket.js';

/**
 * @typedef {import('@bridgething/lib/ts/bindings/server').ServerEvent} ServerEvent
 * @typedef {import('@bridgething/lib/ts/bindings/client').ClientCommand} ClientCommand
 */

const COMMAND_TYPES = ['bluetooth', 'store', 'system', 'voice', 'interaction'];
/**
 * @typedef {Object} TestCommand
 * @property {string} label - The label for the button
 * @property {Object} command - The command object to send
 * @property {ClientCommand['type']} command.type - The type of command
 * @property {ClientCommand['action']} command.action - The action to perform
 */
const TEST_COMMANDS = [
  { label: 'System: Version Request', command: { type: 'system', action: 'versionRequest' } },
  { label: 'System: Gateway Status', command: { type: 'system', action: 'gatewayStatusRequest' } },
  { label: 'Bluetooth: List Devices', command: { type: 'bluetooth', action: 'list' } },
];

/**
 * Initialize and set up the UI components
 * @param {WebSocketClient} client - The WebSocket client instance
 */
export function setupUI(client) {
  setupEventListeners(client);
  setupCollapsiblePanel();
  populateCommandTypeDropdown();
  createTestCommandButtons(client);
}

/**
 * Populate the command type dropdown with valid types
 */
function populateCommandTypeDropdown() {
  const dropdown = document.getElementById('commandType');

  COMMAND_TYPES.forEach(type => {
    const option = document.createElement('option');
    option.value = type;
    option.textContent = type;
    dropdown.appendChild(option);
  });
}

/**
 * Create buttons for test commands based on the TEST_COMMANDS array
 * @param {WebSocketClient} client - The WebSocket client instance
 */
function createTestCommandButtons(client) {
  const commandsContainer = document.getElementById('testCommandsContainer');

  TEST_COMMANDS.forEach(cmd => {
    const button = document.createElement('button');
    button.textContent = cmd.label;
    button.className = 'px-4 py-2 bg-blue-500 text-white rounded hover:bg-blue-600';
    button.addEventListener('click', () => client.send(cmd.command));
    commandsContainer.appendChild(button);
  });
}

/**
 * Set up the collapsible panel for test commands
 */
function setupCollapsiblePanel() {
  const toggleBtn = document.getElementById('toggleCommands');
  const commandsPanel = document.getElementById('commandsPanel');
  const expandIcon = document.getElementById('expandIcon');

  commandsPanel.classList.remove('expanded');

  toggleBtn.addEventListener('click', () => {
    commandsPanel.classList.toggle('expanded');

    if (commandsPanel.classList.contains('expanded')) expandIcon.classList.add('rotate-180');
    else expandIcon.classList.remove('rotate-180');
  });
}

/**
 * Set up event listeners for all interactive UI elements
 * @param {WebSocketClient} client - The WebSocket client instance
 */
function setupEventListeners(client) {
  document.getElementById('reconnectBtn').addEventListener('click', () => client.connect());
  document.getElementById('sendCustomBtn').addEventListener('click', () => sendCustomCommand(client));
  document.getElementById('commandPayload').addEventListener('blur', formatJsonPayload);
}

/**
 * Format the JSON payload when the textarea loses focus
 */
function formatJsonPayload() {
  const payloadEl = document.getElementById('commandPayload');
  const payload = payloadEl.value.trim();

  if (!payload) return;

  try {
    const parsed = JSON.parse(payload);
    payloadEl.value = JSON.stringify(parsed, null, 2);
  } catch (e) {
    console.warn('Invalid JSON payload:', e);
  }
}

/**
 * Send a custom command based on UI inputs
 * @param {WebSocketClient} client - The WebSocket client instance
 */
function sendCustomCommand(client) {
  const type = document.getElementById('commandType').value;
  const action = document.getElementById('commandAction').value.trim();
  const payloadEl = document.getElementById('commandPayload');

  if (!action) return alert('Please enter a command action');

  const command = { type, action };
  const payloadText = payloadEl.value.trim();

  if (payloadText) {
    try {
      const payload = JSON.parse(payloadText);
      Object.assign(command, payload);
    } catch (e) {
      alert('Invalid JSON payload');
      console.error('JSON parse error:', e);
      return;
    }
  }

  client.send(command);
}
