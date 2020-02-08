import React, { } from 'react';
import { ChatRowContent } from './Messages';

const ChatHistoryRow = (props: { content: ChatRowContent }) => {

  return (
    <div className="ChatHistoryRow">
      {props.content.text}
    </div>
  );
}

export default ChatHistoryRow;
