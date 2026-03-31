package com.example.service;

public class EmailGateway {
    public void deliver(String userId, String message) {
        System.out.println("EMAIL to " + userId + ": " + message);
    }
}
