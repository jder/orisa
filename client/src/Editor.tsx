import React from 'react';
import MonacoEditor from 'react-monaco-editor';

export type EditFile = {
  name: string,
  content: string
}

type SaveCallback = () => void;

type ChangeCallback = (content: string) => void;

const Editor = (props: {editFile: EditFile, onSave: SaveCallback, onChange: ChangeCallback}) => {
  
    const { editFile, onSave, onChange } = props;

    const options = {
      selectOnLineNumbers: true
    };

    const handleDidMount = (editor: any) => {
      editor.getModel().updateOptions({ tabSize: 2 })
    }

    return (
      <div className="Editor">
        <div className="header">Editing file {editFile.name}</div>
        <button onClick={onSave}>Save</button>
        <MonacoEditor
          width="800"
          height="600"
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
