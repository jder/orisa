import React, { useState } from 'react';
import MonacoEditor from 'react-monaco-editor';

export type EditFile = {
  name: string,
  content: string
}

type SaveCallback = (file: EditFile) => void;

const Editor = (props: {editFile: EditFile, onSave: SaveCallback}) => {
  
    const { editFile, onSave } = props;

    let [content, setContent] = useState(editFile.content);

    const options = {
      selectOnLineNumbers: true
    };

    const handleChange = (value: string) => {
      setContent(value);
    }

    const handleSave = () => {
      const newFile = {
        name: editFile.name,
        content: content
      };
      onSave(newFile);
    }


    return (
      <div className="Editor">
        <div className="header">Editing file {editFile.name}</div>
        <button onClick={handleSave}>Save</button>
        <MonacoEditor
          width="800"
          height="600"
          language="lua"
          theme="vs-dark"
          value={content}
          options={options}
          onChange={handleChange}
        />
      </div>
    );
}

export default Editor;
