import { v7 as uuid } from 'https://cdn.jsdelivr.net/npm/uuid@11.1.0/dist/esm-browser/index.js';

/**
 * @typedef {import('@bridgething/lib/ts/bindings/client').ClientCommand} ClientCommand
 * @typedef {import('@bridgething/lib/ts/bindings/server').ServerEvent} ServerEvent
 * @typedef {Object} MessageListener
 * @property {(data: string) => void} onMessage
 * @property {() => void} onConnect
 * @property {() => void} onDisconnect
 *
 * @typedef {Object} MessageHandler
 * @property {(message: ServerEvent) => boolean} predicate - Function to determine if handler should process message
 * @property {(message: ServerEvent) => void | Promise<void>} handler - Function to handle the message
 * @property {string} [description] - Optional description of what this handler does
 */

export class WebSocketClient {
  /**
   * @type {WebSocket}
   */
  ws = null;

  /**
   * @type {HTMLElement}
   */
  statusElement = null;

  /**
   * @type {HTMLElement}
   */
  messagesElement = null;

  /**
   * @type {MessageListener[]}
   */
  listeners = [];

  /**
   * @type {MessageHandler[]}
   */
  messageHandlers = [];

  /**
   * @param {string} statusElementId
   * @param {string} messagesElementId
   */
  constructor(statusElementId, messagesElementId) {
    this.statusElement = document.getElementById(statusElementId);
    this.messagesElement = document.getElementById(messagesElementId);
  }

  /**
   * Update the status display
   * @param {string} text
   * @param {string} color
   */
  setStatus(text, color) {
    this.statusElement.textContent = text;
    this.statusElement.style.color = color;
  }

  /**
   * Log a message to the UI
   * @param {'from' | 'to'} direction
   * @param {string} msg
   */
  logMessage(direction, msg) {
    const messageEl = document.createElement('div');
    messageEl.className =
      'my-2 p-3 rounded-lg ' +
      (direction === 'from' ? 'bg-blue-50 border-l-4 border-blue-500' : 'bg-green-50 border-l-4 border-green-500');

    const directionBadge = document.createElement('span');
    directionBadge.className =
      'inline-block px-2 py-1 text-xs font-semibold rounded-full ' +
      (direction === 'from' ? 'bg-blue-100 text-blue-800' : 'bg-green-100 text-green-800');
    directionBadge.textContent = direction === 'from' ? 'RECEIVED' : 'SENT';

    const timestamp = document.createElement('span');
    timestamp.className = 'text-xs text-gray-500 ml-2';
    timestamp.textContent = new Date().toLocaleTimeString();

    const header = document.createElement('div');
    header.className = 'flex items-center mb-1';
    header.appendChild(directionBadge);
    header.appendChild(timestamp);

    const content = document.createElement('div');
    content.className = 'text-sm font-mono break-all whitespace-pre';

    try {
      const parsed = JSON.parse(msg);
      content.textContent = JSON.stringify(parsed, null, 2);
    } catch (e) {
      content.textContent = msg;
    }

    messageEl.appendChild(header);
    messageEl.appendChild(content);

    if (this.messagesElement.firstChild) this.messagesElement.insertBefore(messageEl, this.messagesElement.firstChild);
    else this.messagesElement.appendChild(messageEl);

    this.messagesElement.scrollTop = 0;
  }

  /**
   * Handle incoming WebSocket messages
   * @param {MessageEvent} e
   */
  async handleMessage(e) {
    this.logMessage('from', e.data);
    this.listeners.forEach(listener => listener.onMessage(e.data));

    try {
      /** @type {ServerEvent} */
      const message = JSON.parse(e.data);
      await this.processMessageWithHandlers(message);
    } catch (err) {
      console.error('Error processing message:', err);
    }
  }

  /**
   * Process a message with registered handlers
   * @param {ServerEvent} message - The parsed message object
   */
  async processMessageWithHandlers(message) {
    const matchingHandlers = this.messageHandlers.filter(h => h.predicate(message));

    for (const handler of matchingHandlers) {
      try {
        await handler.handler(message);
      } catch (err) {
        console.error(`Error in message handler "${handler.description || 'unnamed'}"`, err);
      }
    }
  }

  /**
   * Register a new message handler
   * @param {MessageHandler} handler - The handler to register
   * @returns {() => void} A function to remove this handler
   */
  addMessageHandler(handler) {
    this.messageHandlers.push(handler);

    return () => {
      const index = this.messageHandlers.indexOf(handler);
      if (index !== -1) this.messageHandlers.splice(index, 1);
    };
  }

  /**
   * Add a message listener
   * @param {MessageListener} listener
   */
  addListener(listener) {
    this.listeners.push(listener);
  }

  /**
   * Send a generic client command via WebSocket
   * @param {Omit<ClientCommand, 'id'>} msg
   */
  send(msg) {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      console.error('WebSocket is not connected');
      return;
    }

    const str = JSON.stringify({ id: uuid(), ...msg });
    this.ws.send(str);
    this.logMessage('to', str);
    return str;
  }

  /**
   * Connect to the WebSocket server
   * @param {string} url
   */
  connect(url = 'ws://localhost:8891') {
    this.setStatus('connecting', 'orange');

    this.ws = new WebSocket(url);

    this.ws.addEventListener('open', () => {
      this.setStatus('connected', 'green');
      this.send({ type: 'system', action: 'versionRequest' });
      this.listeners.forEach(listener => listener.onConnect());
    });

    this.ws.addEventListener('close', () => {
      this.setStatus('disconnected', 'red');
      this.listeners.forEach(listener => listener.onDisconnect());
    });

    this.ws.addEventListener('message', e => this.handleMessage(e));

    this.ws.addEventListener('error', e => {
      console.error('WebSocket error', e);
      this.setStatus('error', 'red');
    });
  }

  /**
   * Clear all messages from the UI
   */
  clearMessages() {
    while (this.messagesElement.firstChild) this.messagesElement.removeChild(this.messagesElement.firstChild);
  }
}
