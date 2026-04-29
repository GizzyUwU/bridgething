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
  client.on;
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

  client.addListener({
    onConnect: () => {
      setTimeout(() => {
        client.send({
          type: 'forward',
          encoding: 'text',
          data: 'hello from the web client!',
        });
      }, 1000);

      setTimeout(() => {
        client.send({
          type: 'forward',
          encoding: 'json',
          data: { message: 'hello from the web client!' },
        });
      }, 1500);

      setTimeout(async () => {
        client.send({
          type: 'forward',
          encoding: 'binary',
          data: await bufferToBase64(new Uint8Array([69, 69, 69, 69])),
        });
      }, 2000);
    },
  });
}

const bufferToBase64 = buffer =>
  new Promise(r => {
    const reader = new FileReader();
    reader.onload = () => r(typeof reader.result === 'string' ? reader.result : '');
    reader.readAsDataURL(new Blob([buffer]));
    return reader.result;
  }).then(s => s.slice(s.indexOf(',') + 1));

window.bridgethingClient = client;
