import React, { useState } from 'react';
import './App.css';
import ChatPane from './ChatPane';
import { withCookies, useCookies } from 'react-cookie';

const App = () => {
  const [cookies, setCookie] = useCookies(['username']);
  const [newUsername, setNewUsername] = useState("");
  
  const handleSubmit = (event: React.FormEvent) => {
    event.preventDefault();
    setCookie('username', newUsername, { path: '/' });
  }

  const handleChange = (event: React.ChangeEvent<HTMLInputElement>) => {
    setNewUsername(event.target.value);
    event.preventDefault();
  }

  function body(username: string | undefined) {
    if (username) {
      return <ChatPane username={username} />
    } else {
      return <form onSubmit={handleSubmit}>
        <label htmlFor="username">Username: </label>
        <input name="username" autoFocus type="text" value={newUsername} onChange={handleChange} placeholder="frank" />
        <input type="submit" value="Login" />
      </form>
    }
  }

  return (
    <div className="App">
      {body(cookies.username)}
    </div>
  );
}

export default withCookies(App);
