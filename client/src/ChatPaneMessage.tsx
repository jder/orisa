import React, { } from 'react';
import { ServerMessage } from './Messages';

const ChatPaneMessage = (props: { message: ServerMessage }) => {

  return (
    <div className="ChatPaneMessage">
      {props.message.text}
    </div>
  );
}

export default ChatPaneMessage;
