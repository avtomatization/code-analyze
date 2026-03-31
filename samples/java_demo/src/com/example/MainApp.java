package com.example;

import com.example.service.ProjectCoordinator;

public class MainApp {
    public static void main(String[] args) {
        ProjectCoordinator coordinator = new ProjectCoordinator();
        coordinator.run("u-1001", 89.50);
    }
}
