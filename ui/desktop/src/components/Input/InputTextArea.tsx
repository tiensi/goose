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
    className={`w-full min-h-[1rem] max-h-[240px] overflow-y-auto outline-none border-none focus:ring-0 bg-transparent text-14 resize-none pr-3 ${
      disabled ? 'cursor-not-allowed opacity-50' : ''
    }`}
  />
);