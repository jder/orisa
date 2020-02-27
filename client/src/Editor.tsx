import React from 'react';
import MonacoEditor from 'react-monaco-editor';
import './Editor.css';

export type EditFile = {
  name: string,
  content: string
}

type SaveCallback = () => void;

type ChangeCallback = (content: string) => void;

type CloseCallback = () => void;

const Editor = (props: {editFile: EditFile, onSave: SaveCallback, onChange: ChangeCallback, onClose: CloseCallback}) => {
  
    const { editFile, onSave, onChange, onClose } = props;

    const options = {
      selectOnLineNumbers: true
    };

    const handleDidMount = (editor: any) => {
      editor.getModel().updateOptions({ tabSize: 2 })
    }

    return (
      <div className="Editor">
        <div className="header">
          <h2>Editing file <strong>{editFile.name}</strong></h2>
          <button onClick={onSave}>Save</button>
          <button className="close" onClick={onClose}>Cancel</button>
        </div>
        <MonacoEditor
          height="100%"
          language="lua"
          theme="vs-dark"
          value={editFile.content}
          editorDidMount={handleDidMount}
          options={options}
          onChange={onChange}
        />
      </div>
    );
}

export default Editor;
