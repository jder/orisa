import React, { useState, useEffect } from 'react';
import './App.css';
import ChatPane from './ChatPane';
import { ServerMessage } from './Messages';
import { ChatSocket } from './ChatSocket';
import uuid from 'uuid/v4';

const App = () => {
  const [text, setText] = useState("");
  const [messages, setMessages] = useState([] as ServerMessage[]);
  const [socket, setSocket] = useState(null as ChatSocket | null);

  useEffect(() => {
    const loc = document.location;
    const protocol = loc.protocol === 'https' ? 'wss' : 'ws';
    let s = new ChatSocket(`${protocol}://${loc.host}/api/socket`);
    s.onmessage = (message: ServerMessage) => {
      setMessages(prev => prev.concat([message]));
    };
    setSocket(s);
  }, [])

  const handleSubmit = (event: React.FormEvent) => {
    event.preventDefault();

    socket!.send({text: text, id: uuid()});
    setText("");
  }

  const handleChange = (event: React.ChangeEvent<HTMLInputElement>) => {
    setText(event.target.value);
    event.preventDefault();
  }

  return (
    <div className="App">
      <ChatPane messages={messages} />   
      <form onSubmit={handleSubmit}>
        <input className="mainInput" autoFocus type="text" value={text} onChange={handleChange} />
        <input type="submit" disabled={!socket} value="Send" />
      </form>
    </div>
  );
}

export default App;
