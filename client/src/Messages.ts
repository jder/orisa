// From server
export class ToServerMessage {
  type: string;
  
  constructor(type: string) {
    this.type = type;
  }
}

export class LoginMessage extends ToServerMessage {
  username: string;

  constructor(username: string) {
    super("Login")
    this.username = username;
  }
}

export class SayMessage extends ToServerMessage {
  text: string;

  constructor(text: string) {
    super("Say")
    this.text = text;
  }
}

export type ToClientMessage = { type: string; };
export type TellMessage = {type: string, content: ChatRowContent};
export type BacklogMessage = {type: string, history: [ChatRowContent] };

export function isTellMessage(m: ToClientMessage): m is TellMessage {
  return m.type == "Tell";
}

export function isBacklogMessage(m: ToClientMessage): m is BacklogMessage {
  return m.type == "Backlog";
}

// A row to display. Someday could include HTML, actions, styling, etc.
export class ChatRowContent {
  id: string;
  text: string;

  constructor(id: string, text: string) {
    this.id = id;
    this.text = text;
  }
}