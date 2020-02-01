import React, { } from 'react';
import { ServerMessage } from './Messages';
import ChatPaneMessage from './ChatPaneMessage';

const ChatPane = (props: { messages: ServerMessage[] }) => {

  return (
    <div className="ChatPane">
      {props.messages.map((m) => <ChatPaneMessage message={m} key={m.id} />)}
    </div>
  );
}

export default ChatPane;
