import React, { useState, useEffect } from 'react';
import ChatHistory from './ChatHistory';
import { ToClientMessage, isTellMessage, isBacklogMessage, ChatRowContent, SayMessage } from './Messages';
import { ChatSocket } from './ChatSocket';
import uuid from 'uuid/v4';

const ChatPane = (props: {username: string}) => {
  const [text, setText] = useState("");
  const [rows, setRows] = useState([] as ChatRowContent[]);
  const [socket, setSocket] = useState(null as ChatSocket | null);

  useEffect(() => {
    const loc = document.location;
    const protocol = loc.protocol === 'https' ? 'wss' : 'ws';
    let s = new ChatSocket(`${protocol}://${loc.host}/api/socket`, props.username);
    s.onmessage = (message: ToClientMessage) => {
      setRows((prev) => {
        if (isTellMessage(message)) {
          return prev.concat([message.content]);
        } else if (isBacklogMessage(message)) {
          return message.history;
        } else {
          console.error("Unrecognized message", message);
          return prev;
        }
      });
    };
    setSocket(s);
  }, [])

  const handleSubmit = (event: React.FormEvent) => {
    event.preventDefault();

    socket!.send(new SayMessage(text));
    setText("");
  }

  const handleChange = (event: React.ChangeEvent<HTMLInputElement>) => {
    setText(event.target.value);
    event.preventDefault();
  }

  return (
    <div className="ChatPane">
      <ChatHistory rows={rows} />   
      <form onSubmit={handleSubmit}>
        <input className="mainInput" autoFocus type="text" value={text} onChange={handleChange} />
        <input type="submit" disabled={!socket} value="Send" />
      </form>
    </div>
  );
}

export default ChatPane;
