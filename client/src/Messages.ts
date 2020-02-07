// From server
export class ToServerMessage {
  type: String;
  
  constructor(type: String) {
    this.type = type;
  }
}

export class LoginMessage extends ToServerMessage {
  username: String;

  constructor(username: String) {
    super("Login")
    this.username = username;
  }
}

export class SayMessage extends ToServerMessage {
  text: String;

  constructor(text: String) {
    super("Say")
    this.text = text;
  }
}

export class ToClientMessage {
  type: String;
  
  constructor(type: String) {
    this.type = type;
  }
}

export class TellMessage extends ToClientMessage {
  text: String;

  constructor(text: String) {
    super("Tell");
    this.text = text;
  }
}

export class BacklogMessage extends ToClientMessage {
  history: [String];

  constructor(history: [String]) {
    super("Backlog");
    this.history = history;
  }

}