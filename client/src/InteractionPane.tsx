import React, { useState, useEffect } from 'react';
import ChatHistory from './ChatHistory';
import { ToClientMessage, isTellMessage, isBacklogMessage, ChatRowContent, CommandMessage, ReloadCodeMessage, isLogMessage, SaveFileMessage, isEditFileMessage } from './Messages';
import { ChatSocket } from './ChatSocket';
import Editor, { EditFile } from './Editor';
import './InteractionPane.css';

const InteractionPane = (props: {username: string}) => {
  const [text, setText] = useState("");
  const [lastText, setLastText] = useState("");
  const [rows, setRows] = useState([] as ChatRowContent[]);
  const [socket, setSocket] = useState(null as ChatSocket | null);
  const [editFile, setEditFile] = useState(null as EditFile | null);

  const mainInputRef:any = React.createRef();

  useEffect(() => {
    const loc = document.location;
    const protocol = loc.protocol === 'https:' ? 'wss' : 'ws';
    let s = new ChatSocket(`${protocol}://${loc.host}/api/socket`, props.username);
    s.onmessage = (message: ToClientMessage) => {
      setRows((prev) => {
        if (isTellMessage(message)) {
          return prev.concat([message.content]);
        } else if (isBacklogMessage(message)) {
          return message.history;
        } else if (isLogMessage(message)) {
          if (message.level === "error") {
            console.error(message.message);
          } else {
            console.log(message.message);
          }
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
  }, [props.username])

  const handleSubmit = (event: React.FormEvent) => {
    event.preventDefault();

    socket!.send(new CommandMessage(text));
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

  const handleEditClose = () => {
    setEditFile(null);
    mainInputRef.current.focus();
  }

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.keyCode === 38) {
      // up arrow
      setText(lastText);
      e.preventDefault();
    }
  }

  const handleHistoryClick = (e: React.MouseEvent) => {
    // if the user clicks the chat history, focus the input box, UNLESS
    // the user is trying to select text!
    const selection = window.getSelection();
    if (selection && selection.type === 'Range') return;
    mainInputRef.current.focus();
  }

  return (
    <div className="InteractionPane">
      <ChatHistory rows={rows} onClick={handleHistoryClick} /> 
      <form className="input-bar" onSubmit={handleSubmit}>
        <div>&gt;&emsp;</div>
        <input className="mainInput" autoFocus type="text" value={text} onChange={handleChange} onKeyDown={handleKeyDown} ref={mainInputRef} />
        <input type="submit" disabled={!socket} value="Send" />
      </form>
      <div className="tool-bar">
        <button onClick={handleReload}>Reload System Code</button>
      </div>

      {editFile && <Editor editFile={editFile} onSave={handleEditSave} onChange={handleEditChange} onClose={handleEditClose} /> }
    </div>
  );
}

export default InteractionPane;
