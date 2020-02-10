import React, { useState, useEffect } from 'react';
import ChatHistory from './ChatHistory';
import { ToClientMessage, isTellMessage, isBacklogMessage, ChatRowContent, SayMessage, ReloadCodeMessage, isLogMessage, SaveFileMessage, isEditFileMessage } from './Messages';
import { ChatSocket } from './ChatSocket';
import Editor, { EditFile } from './Editor'; 

const InteractionPane = (props: {username: string}) => {
  const [text, setText] = useState("");
  const [lastText, setLastText] = useState("");
  const [rows, setRows] = useState([] as ChatRowContent[]);
  const [socket, setSocket] = useState(null as ChatSocket | null);
  const [editFile, setEditFile] = useState(null as EditFile | null);

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
        } else if (isLogMessage(message)) {
          console.error(message.message);
          return prev;
        } else if (isEditFileMessage(message)) {
          setEditFile(message);
          return prev;
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
    setLastText(text);
    setText("");
  }

  const handleChange = (event: React.ChangeEvent<HTMLInputElement>) => {
    setText(event.target.value);
    event.preventDefault();
  }

  const handleReload = () => {
    socket!.send(new ReloadCodeMessage())
  }

  const handleEditSave = () => {
    if (editFile) {
      socket!.send(new SaveFileMessage(editFile.name, editFile.content))
    }
  }

  const handleEditChange = (content: string) => {
    if (editFile) {
      setEditFile({name: editFile?.name, content: content})
    }
  }

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.keyCode === 38) {
      // up arrow
      setText(lastText);
      e.preventDefault();
    }
  }

  return (
    <div className="InteractionPane">
      <ChatHistory rows={rows} /> 
      <form onSubmit={handleSubmit}>
        <input className="mainInput" autoFocus type="text" value={text} onChange={handleChange} onKeyDown={handleKeyDown} />
        <input type="submit" disabled={!socket} value="Send" />
      </form>
      <button onClick={handleReload}>Reload System Code</button>
      {editFile && <Editor editFile={editFile} onSave={handleEditSave} onChange={handleEditChange} /> }
    </div>
  );
}

export default InteractionPane;
