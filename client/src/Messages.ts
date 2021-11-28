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

export class CommandMessage extends ToServerMessage {
  text: string;
  extra: JSON | null;

  constructor(text: string, extra: JSON | null = null) {
    super("Command")
    this.text = text;
    this.extra = extra;
  }
}

export class ReloadCodeMessage extends ToServerMessage {
  constructor() {
    super("ReloadCode")
  }
}

export class SaveFileMessage extends ToServerMessage {
  name: string;
  content: string;

  constructor(name: string, content: string) {
    super("SaveFile")
    this.name = name;
    this.content = content;
  }
}

export type ToClientMessage = { type: string; };
export type TellMessage = { type: string, content: ChatRowContent };
export type BacklogMessage = { type: string, history: [ChatRowContent] };
export type LogMessage = { type: string, message: string, level: string };
export type EditFileMessage = { type: string, name: string, content: string };

export function isTellMessage(m: ToClientMessage): m is TellMessage {
  return m.type === "Tell";
}

export function isBacklogMessage(m: ToClientMessage): m is BacklogMessage {
  return m.type === "Backlog";
}

export function isLogMessage(m: ToClientMessage): m is LogMessage {
  return m.type === "Log";
}

export function isEditFileMessage(m: ToClientMessage): m is EditFileMessage {
  return m.type === "EditFile";
}

export type ChatRowContent = { id: string, text: string } | { id: string, html: string };