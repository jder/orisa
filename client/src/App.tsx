import React, { useState } from 'react';
import './App.css';
import ChatPane from './ChatPane';

const App = () => {
  const [text, setText] = useState("");
  const [messages, setMessages] = useState([]);

  const handleSubmit = (event: React.FormEvent) => {
    event.preventDefault();

    const submitted = text;
    setText("");
  }

  const handleChange = (event: React.ChangeEvent<HTMLInputElement>) => {
    setText(event.target.value);
    event.preventDefault();
  }

  return (
    <div className="App">
      <ChatPane message={messages} />   
      <form onSubmit={handleSubmit}>
        <input type="text" value={text} onChange={handleChange} />
      </form>
    </div>
  );
}

export default App;
