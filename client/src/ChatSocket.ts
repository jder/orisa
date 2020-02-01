import { ServerMessage, ClientMessage } from './Messages';

export class ChatSocket {
  private url: string;
  private ws: WebSocket;

  onmessage?: (message: ServerMessage) => void;
  buffer: ClientMessage[] = [];

  constructor(url: string) {
    this.url = url;
    this.ws = this.connect();
  }

  private connect() {
    this.ws = new WebSocket(this.url);
    this.ws.onmessage = (event: MessageEvent) => {
      if (this.onmessage) {
        const parsed = JSON.parse(event.data);
        console.log("got message", parsed);
        this.onmessage(parsed as ServerMessage);
      }
    }
    this.ws.onerror = (event: Event) => {
      console.error("websocket error", event);
      setTimeout(() => this.connect(), 5000);
    }
    this.ws.onopen = (event: Event) => {
      console.log("websocket open");
      const toSend = this.buffer;
      this.buffer = [];

      toSend.forEach(message => {
        this.send(message);
      });
    }
    return this.ws;
  }

  send(message: ClientMessage) {
    if (this.ws.readyState !== WebSocket.OPEN) {
      console.warn("Tried to send message when websocket is closed; buffering");
      this.buffer.push(message);
    } else {
      console.log("Sending message", message);
      this.ws.send(JSON.stringify(message));
    }
  }
}