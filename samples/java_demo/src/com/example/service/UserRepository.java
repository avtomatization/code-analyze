package com.example.service;

import com.example.model.User;

public class UserRepository {
    public User findById(String userId) {
        if ("u-1001".equals(userId)) {
            return new User(userId, 150.0);
        }
        return null;
    }
}
