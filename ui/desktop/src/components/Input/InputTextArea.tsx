import React from 'react';

interface InputTextAreaProps {
  text: string;
  disabled: boolean;
  textAreaRef: React.RefObject<HTMLTextAreaElement>;
  handleChange: (evt: React.ChangeEvent<HTMLTextAreaElement>) => void;
  handleKeyDown: (evt: React.KeyboardEvent<HTMLTextAreaElement>) => void;
}

export const InputTextArea = ({
  text,
  disabled,
  textAreaRef,
  handleChange,
  handleKeyDown
}: InputTextAreaProps) => (
  <textarea
    autoFocus
    id="dynamic-textarea"
    placeholder="What should goose do?"
    value={text}
    onChange={handleChange}
    onKeyDown={handleKeyDown}
    disabled={disabled}
    ref={textAreaRef}
    rows={1}
    style={{
      minHeight: '1rem',
      maxHeight: '240px',
      overflowY: 'auto'
    }}
    className={`w-full outline-none border-none focus:ring-0 bg-transparent p-0 text-14 resize-none ${
      disabled ? 'cursor-not-allowed opacity-50' : ''
    }`}
  />
);