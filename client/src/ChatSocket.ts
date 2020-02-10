import { ToServerMessage, ToClientMessage, LoginMessage } from './Messages';

export class ChatSocket {
  private url: string;
  private ws: WebSocket;
  private username: string;

  onmessage?: (message: ToClientMessage) => void;
  buffer: ToServerMessage[] = [];

  constructor(url: string, username: string) {
    this.url = url;
    this.ws = this.connect();
    this.username = username;
  }

  private connect() {
    this.ws = new WebSocket(this.url);
    this.ws.onmessage = (event: MessageEvent) => {
      if (this.onmessage) {
        const parsed = JSON.parse(event.data);
        console.debug("got message", parsed);
        this.onmessage(parsed as ToClientMessage);
      }
    }
    this.ws.onerror = (event: Event) => {
      console.error("websocket error", event);
    }
    this.ws.onclose = (event: CloseEvent) => {
      setTimeout(() => this.connect(), 5000);
    }
    this.ws.onopen = (event: Event) => {
      console.debug("websocket open");

      this.send(new LoginMessage(this.username));

      const toSend = this.buffer;
      this.buffer = [];

      toSend.forEach(message => {
        this.send(message);
      });
    }
    return this.ws;
  }

  send(message: ToServerMessage) {
    if (this.ws.readyState !== WebSocket.OPEN) {
      console.warn("Tried to send message when websocket is closed; buffering");
      this.buffer.push(message);
    } else {
      console.debug("Sending message", message);
      this.ws.send(JSON.stringify(message));
    }
  }
}