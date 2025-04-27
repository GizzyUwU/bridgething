import { setupUI } from './ui.js';
import { WebSocketClient } from './websocket.js';

/**
 * @typedef {import('@bridgething/lib/ts/bindings/server').ServerEvent} ServerEvent
 * @typedef {import('@bridgething/lib/ts/bindings/client').ClientCommand} ClientCommand
 */

// Initialize the WebSocket client
const client = new WebSocketClient('status', 'messages');

// Connect when the page loads
document.addEventListener('DOMContentLoaded', () => {
  registerMessageHandlers();
  client.connect();
  setupUI(client);
});

/**
 * Register all message handlers for programmatic responses
 */
function registerMessageHandlers() {
  client.addMessageHandler({
    description: 'version response handler',
    predicate: message => message.type === 'system' && message.data?.action === 'version',
    handler: message => {
      console.log('received version response:', message);
      // add responses here
    },
  });
}

window.bridgethingClient = client;
