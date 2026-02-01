/**
 * @file PID_v1.h
 * @brief PID library stub for native test environment
 * 
 * Provides minimal PID class stub for testing without actual PID library
 */

#pragma once

// PID mode constants
#define AUTOMATIC 1
#define MANUAL 0
#define DIRECT 0
#define REVERSE 1
#define P_ON_M 1
#define P_ON_E 0

// Minimal PID class stub
class PID {
public:
    PID(double* input, double* output, double* setpoint,
        double kp, double ki, double kd, int pOn, int controllerDirection) 
        : input_(input), output_(output), setpoint_(setpoint),
          kp_(kp), ki_(ki), kd_(kd), mode_(AUTOMATIC) {}
    
    virtual ~PID() = default;
    
    bool Compute() {
        if (mode_ == AUTOMATIC && input_ && output_ && setpoint_) {
            double error = *setpoint_ - *input_;
            // Simple PID computation for testing
            static double integral = 0.0;
            static double lastError = 0.0;
            
            integral += error;
            double derivative = error - lastError;
            
            *output_ = kp_ * error + ki_ * integral + kd_ * derivative;
            
            // Clamp output
            if (*output_ > 1000.0) *output_ = 1000.0;
            if (*output_ < 0.0) *output_ = 0.0;
            
            lastError = error;
            return true;
        }
        return false;
    }
    
    void SetMode(int mode) { mode_ = mode; }
    void SetTunings(double kp, double ki, double kd, int pOn = 1) {
        kp_ = kp;
        ki_ = ki;
        kd_ = kd;
    }
    void SetOutputLimits(double min, double max) {
        minOutput_ = min;
        maxOutput_ = max;
    }
    void SetIntegratorLimits(double min, double max) {
        minIntegral_ = min;
        maxIntegral_ = max;
    }
    void SetSampleTime(int sampleTime) {
        sampleTime_ = sampleTime;
    }
    void SetSmoothingFactor(double factor) {
        smoothingFactor_ = factor;
    }
    
    int GetMode() const { return mode_; }
    double GetKp() const { return kp_; }
    double GetKi() const { return ki_; }
    double GetKd() const { return kd_; }
    double GetLastPPart() const { return 0.0; }
    double GetLastIPart() const { return 0.0; }
    double GetLastDPart() const { return 0.0; }
    double GetInputError() const { return 0.0; }
    double GetDeltaInput() const { return 0.0; }
    
private:
    double* input_;
    double* output_;
    double* setpoint_;
    double kp_, ki_, kd_;
    int mode_;
    double minOutput_ = 0.0;
    double maxOutput_ = 1000.0;
    double minIntegral_ = 0.0;
    double maxIntegral_ = 50.0;
    int sampleTime_ = 100;
    double smoothingFactor_ = 0.0;
};
