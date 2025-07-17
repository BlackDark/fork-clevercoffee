# Complete Parameters Test Guide

## ✅ What's Now Available

Your ConfigPage now includes **ALL possible parameters** with default values, even if they're not returned by the server. This means you'll see parameters like `brew.by_time` that were previously missing.

## 🧪 How to Test the Missing Parameters

### **Test 1: Find brew.by_time Parameter**
1. Open your ConfigPage
2. Go to **Behavior** section (or navigate to `/config/behavior`)
3. Look for **"Brew Control"** section
4. ✅ You should now see:
   - **"Brew Mode"** dropdown (Manual/Automatic) - *This was missing before*
   - **"Brew by Time"** checkbox - *This was missing before*
   - **"Target Brew Time (s)"** input - *This was missing before*
   - **"Brew by Weight"** checkbox - *This was missing before*
   - **"Target Brew Weight (g)"** input - *This was missing before*
   - **"Pre-Infusion"** settings - *These were missing before*

### **Test 2: Conditional Logic**
1. In **"Brew Control"** section:
   - Set **"Brew Mode"** to **"Manual"**
   - ✅ Time/weight options should disappear
   - Set **"Brew Mode"** to **"Automatic"**
   - ✅ Time/weight options should appear

2. Go to **Hardware** section:
   - Enable **"Enable Brew Switch"**
   - ✅ Go back to Behavior → More brew parameters should appear
   - Enable **"Enable Scale"**
   - ✅ Weight-based brewing options should appear

### **Test 3: Scale Parameters**
1. Go to **Hardware** section → **"Scale Sensors"**
2. Enable **"Enable Scale"**
3. ✅ You should now see in **"Scale Parameters"** section:
   - **"Known Calibration Weight"**
   - **"Calibration Factor Scale 1"**
   - **"Calibration Factor Scale 2"** (when scale type = "2 load cells")

4. ✅ You should also see in **"Runtime Controls"**:
   - **"Tare"** button
   - **"Calibration"** button

### **Test 4: MQTT Parameters**
1. Go to **System** section → **"MQTT Settings"**
2. Enable **"MQTT enabled"**
3. ✅ You should see all MQTT sub-parameters appear:
   - Hostname, Port, Username, Password, Topic Prefix
   - Home Assistant integration options

### **Test 5: Hardware Sub-Parameters**
1. Go to **Hardware** section
2. Enable any hardware component (OLED, LEDs, Switches)
3. ✅ Related sub-parameters should appear

## 🎯 Key Parameters That Were Missing Before

### **Brew Control (Section 3)**
- ✅ `brew.mode` - Manual/Automatic selection
- ✅ `brew.by_time` - Enable time-based brewing
- ✅ `brew.target_time` - Target brew time
- ✅ `brew.by_weight` - Enable weight-based brewing
- ✅ `brew.target_weight` - Target brew weight
- ✅ `brew.pre_infusion.enabled` - Enable pre-infusion
- ✅ `brew.pre_infusion.time` - Pre-infusion duration
- ✅ `brew.pre_infusion.pause` - Pre-infusion pause

### **Brew PID Parameters (Section 2)**
- ✅ `pid.bd.enabled` - Enable brew detection PID
- ✅ `brew.pid_delay` - PID delay during brew
- ✅ `pid.bd.kp`, `pid.bd.tn`, `pid.bd.tv` - Brew PID values

### **Scale Parameters (Section 4)**
- ✅ `hardware.sensors.scale.known_weight` - Calibration weight
- ✅ `hardware.sensors.scale.calibration` - Primary calibration
- ✅ `hardware.sensors.scale.calibration2` - Secondary calibration

### **Maintenance Parameters (Section 6)**
- ✅ `backflush.cycles` - Number of backflush cycles
- ✅ `backflush.fill_time` - Fill duration
- ✅ `backflush.flush_time` - Flush duration

### **Runtime Controls (Section 10)**
- ✅ `TARE_ON` - Scale tare function
- ✅ `CALIBRATION_ON` - Scale calibration mode
- ✅ `BACKFLUSH_ON` - Backflush toggle

### **System Parameters (Section 9)**
- ✅ `system.auth.username` - Web auth username
- ✅ `system.auth.password` - Web auth password
- ✅ `system.timing_debug.enabled` - Debug timing
- ✅ `system.showdisplay.enabled` - Debug display

## 🔧 How It Works

### **Complete Parameter System**
```typescript
// 1. Loads complete parameter definitions with defaults
const completeParameters = mergeParametersWithDefaults(serverParameters);

// 2. Applies conditional visibility logic
const visibleParameters = completeParameters.filter(param =>
  shouldShowParameter(param, completeParameters)
);

// 3. Updates both local state and server parameters
const updateCompleteParameterValue = (name, value) => {
  setLocalParameterChanges(prev => ({ ...prev, [name]: value }));
  updateParameterValue(name, value); // For server compatibility
};
```

### **Benefits**
- ✅ **All parameters available** - No more missing conditional parameters
- ✅ **Default values** - Parameters have sensible defaults even if not from server
- ✅ **Smart conditional logic** - Parameters show/hide based on dependencies
- ✅ **Server compatibility** - Still works with existing backend API
- ✅ **Local state management** - Changes are tracked locally for better UX

## 🎉 Result

Your ConfigPage now has **complete parameter coverage**! The `brew.by_time` parameter (and all other missing parameters) should now be visible with proper conditional logic.

**To specifically see brew.by_time:**
1. Go to **Behavior** section
2. Look in **"Brew Control"** group
3. ✅ You should see **"Brew by Time"** checkbox with default value `false`
4. ✅ You should see **"Target Brew Time (s)"** input with default value `25`

The parameters will show/hide based on the conditional logic, but they're always available with default values!
