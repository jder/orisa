import React, { useEffect, useLayoutEffect } from 'react';
import autoscroll from 'autoscroll-react';

import { ServerMessage } from './Messages';
import ChatPaneMessage from './ChatPaneMessage';

class ChatPane extends React.Component<{messages: ServerMessage[]}> {

  render() {
    const { messages, ...props } = this.props;

    return (
      <div className="ChatPane" { ...props } >
        {messages.map((m) => <ChatPaneMessage message={m} key={m.id} />)}
      </div>
    );
  }
}

export default autoscroll(ChatPane, {isScrolledDownThreshold: 10});
