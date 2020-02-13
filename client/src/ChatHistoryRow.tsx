import React, { } from 'react';
import dompurify from 'dompurify';
import { ChatRowContent } from './Messages';

const ChatHistoryRow = (props: { content: ChatRowContent }) => {


  if ('text' in props.content) {
    return (
      <div className="ChatHistoryRow">
        {props.content.text}
      </div>
    );
  } else {
    return (
      <div className="ChatHistoryRow" dangerouslySetInnerHTML={{__html: dompurify.sanitize(props.content.html) }} />
    )
  }

}

export default ChatHistoryRow;
