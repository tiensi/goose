"use client"

import React, { useState } from 'react';
import { Button } from "../../ui/button"
import {
    Modal,
    ModalContent,
    ModalDescription,
    ModalHeader,
    ModalTitle,
    ModalTrigger,
} from "../../ui/modal"
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from "../../ui/select"
import { Input } from "../../ui/input"
import { Label } from "../../ui/label"

export function AddModelDialog() {
    const [selectedValue, setSelectedValue] = useState<string | null>(null);
    const [isOpen, setIsOpen] = useState(false);
    return (
        <Modal>
            <ModalTrigger asChild>
                <Button className="bg-blue-500 hover:bg-blue-600 text-white">
                    Add Model
                </Button>
            </ModalTrigger>
            <ModalContent className="sm:max-w-[425px]">
                <ModalHeader>
                    <ModalTitle>Add New Model</ModalTitle>
                    <ModalDescription>
                        Add a new model by selecting a provider and entering the model name.
                    </ModalDescription>
                </ModalHeader>
                <div className="grid gap-4 py-4">
                    <div className="grid gap-2">
                        <Label htmlFor="provider">Provider</Label>
                        <Select>
                            <SelectTrigger onClick={() => setIsOpen(!isOpen)}>
                                <SelectValue placeholder="Select provider" />
                            </SelectTrigger>
                            <SelectContent isOpen={isOpen}>
                                <SelectItem value="openai" onSelect={setSelectedValue}>
                                    OpenAI
                                </SelectItem>
                                <SelectItem value="anthropic" onSelect={setSelectedValue}>
                                    Anthropic
                                </SelectItem>
                                <SelectItem value="google" onSelect={setSelectedValue}>
                                    Google
                                </SelectItem>
                                <SelectItem value="mistral" onSelect={setSelectedValue}>
                                    Mistral
                                </SelectItem>
                                <SelectItem value="amazon" onSelect={setSelectedValue}>
                                    Amazon
                                </SelectItem>
                                <SelectItem value="azure" onSelect={setSelectedValue}>
                                    Azure
                                </SelectItem>
                            </SelectContent>
                        </Select>
                    </div>
                    <div className="grid gap-2">
                        <Label htmlFor="model">Model Name</Label>
                        <Input id="model" placeholder="Enter model name" />
                    </div>
                </div>
                <div className="flex justify-end gap-3">
                    <Button variant="outline">Cancel</Button>
                    <Button className="bg-blue-500 hover:bg-blue-600 text-white">
                        Add Model
                    </Button>
                </div>
            </ModalContent>
        </Modal>
    )
}

