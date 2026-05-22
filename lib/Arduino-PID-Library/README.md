**Arduino PID Library with added features:**

* Low-pass filtering (using an exponentially weighted moving average filter) of the input before deriving it for much less noisy D part of the controller. Use the new SetSmoothingFactor() method to change the filter strength.

* Allow settings separate limits for the internal integrator (using the new SetIntegratorLimits() method)

* More methods to get PID internal values for debugging (GetPonE(), GetDeltaInput(), GetLastPPart(), GetLastIPart(), GetLastDPart(), GetInputError())


**Original Readme**

	Arduino PID Library - Version 1.2.1
	by Brett Beauregard <br3ttb@gmail.com> brettbeauregard.com
	
	This Library is licensed under the MIT License

 - For an ultra-detailed explanation of why the code is the way it is, please visit: 
   http://brettbeauregard.com/blog/2011/04/improving-the-beginners-pid-introduction/

 - For function documentation see:  http://playground.arduino.cc/Code/PIDLibrary
