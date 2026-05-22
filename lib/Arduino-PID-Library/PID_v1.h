#ifndef PID_v1_h
#define PID_v1_h
#define LIBRARY_VERSION 1.2.1

class PID {
// Constants used in some of the functions below
#define AUTOMATIC 1
#define MANUAL    0
#define DIRECT    0
#define REVERSE   1
#define P_ON_M    0
#define P_ON_E    1

  public:
    // commonly used functions **************************************************************************
    PID(double*,
        double*,
        double*, // * constructor.  links the PID to the Input, Output, and
        double,
        double,
        double,
        int,
        int); //   Setpoint.  Initial tuning parameters are also set here.
              //   (overload for specifying proportional mode)

    PID(double*,
        double*,
        double*, // * constructor.  links the PID to the Input, Output, and
        double,
        double,
        double,
        int);       //   Setpoint.  Initial tuning parameters are also set here

    bool Compute(); // * performs the PID calculation.  it should be
                    //   called every time loop() cycles. ON/OFF and
                    //   calculation frequency can be set using SetMode
                    //   SetSampleTime respectively

    // Setters
    void SetMode(int Mode);                   // * sets PID to either Manual (0) or Auto (non-0)

    void SetOutputLimits(double, double);     // * clamps the output to a specific range. 0-255 by default, but
                                              //   it's likely the user will want to change this depending on
                                              //   the application

    void SetIntegratorLimits(double, double); // * clamps the integrator to a specific range. 0-255 by default, but
                                              //   a smaller value than the output might be useful for anti-windup

    // available but not commonly used functions ********************************************************
    void SetTunings(double,
                    double,  // * While most users will set the tunings once in the
                    double); //   constructor, this function gives the user the option
                             //   of changing tunings during runtime for Adaptive control
    void SetTunings(double,
                    double,  // * overload for specifying proportional mode
                    double,
                    int);

    void SetControllerDirection(int); // * Sets the Direction, or "Action" of the controller. DIRECT
                                      //   means the output will increase when error is positive. REVERSE
                                      //   means the opposite.  it's very unlikely that this will be needed
                                      //   once it is set in the constructor.
    void SetSampleTime(int);          // * sets the frequency, in Milliseconds, with which
                                      //   the PID calculation is performed.  default is 100

    // Set smoothing factor for input low pass filtering (e.g. 0.9, the higher, the more filtering)
    void SetSmoothingFactor(double alpha);

    // Getters
    double GetKp();         // These functions query the pid for interal values.
    double GetKi();         //  they were created mainly for the pid front-end,
    double GetKd();         // where it's important to know what is actually
    int    GetMode();       //  inside the PID.
    int    GetDirection();  //
    bool   GetPonE();
    double GetDeltaInput(); // Get dInput used for calculating D term
    double GetLastPPart();  // Get internal PID integrator value
    double GetLastIPart();  // Get internal PID integrator value
    double GetLastDPart();  // Get internal PID integrator value
    double GetInputError();

  private:
    void Initialize();

    double dispKp; // * we'll hold on to the tuning parameters in user-entered
    double dispKi; //   format for display purposes
    double dispKd; //

    double kp;     // * (P)roportional Tuning Parameter
    double ki;     // * (I)ntegral Tuning Parameter
    double kd;     // * (D)erivative Tuning Parameter

    int controllerDirection;
    int pOn;

    double* myInput;    // * Pointers to the Input, Output, and Setpoint variables
    double* myOutput;   //   This creates a hard link between the variables and the
    double* mySetpoint; //   PID, freeing the user from having to constantly tell us
                        //   what these values are.  with pointers we'll just know.

    unsigned long lastTime;
    double        integrator; // Integrator sum used in compute loop method

    // filter smoothing factor: roughly, the higher the value, the lower are the allowed frequencies to pass (but the
    // longer the delay for changes to have an effect)
    double filterAlpha = 0.9;

    double lastInput;
    double lastFilteredInput;
    double lastFilteredDifferential;
    double lastError;
    double lastPPart;
    double lastDPart;

    unsigned long SampleTime;
    double        outMin, outMax;
    double        integratorMin, integratorMax;
    bool          inAuto, pOnE;
};

#endif
