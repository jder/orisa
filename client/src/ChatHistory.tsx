import React from 'react';
import autoscroll from 'autoscroll-react';

import { ChatRowContent } from './Messages';
import ChatHistoryRow from './ChatHistoryRow';

class ChatHistory extends React.Component<{rows: ChatRowContent[]}> {

  render() {
    const { rows, ...props } = this.props;

    return (
      <div className="ChatHistory" { ...props } >
        {rows.map((m) => <ChatHistoryRow content={m} key={m.id} />)}
      </div>
    );
  }
}

export default autoscroll(ChatHistory, {isScrolledDownThreshold: 10});
