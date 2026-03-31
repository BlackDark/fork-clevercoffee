import { OTAUpdateSection } from "@/components/OTAUpdateSection";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { useCleverCoffee } from "@/context/useCleverCoffee";
import { API_BASE_URL, apiFetch } from "@/lib/api-config";
import {
  AlertCircle,
  Download,
  Loader2,
  Moon,
  Power,
  RefreshCw,
  TriangleAlert,
  Upload,
  Wifi,
  Zap,
} from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

export function SystemPage() {
  const [selectedFile, setSelectedFile] = useState<File | null>(null);
  const [isUploading, setIsUploading] = useState(false);
  const [uploadMessage, setUploadMessage] = useState("");
  const [uploadSuccess, setUploadSuccess] = useState(false);

  // Use the centralized hook for connection error handling
  const { connectionError, wakeFromStandby, sleepFromStandby } = useCleverCoffee();

  const formatFileSize = (bytes: number) => {
    if (bytes === 0) return "0 Bytes";
    const k = 1024;
    const sizes = ["Bytes", "KB", "MB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
  };

  const restartMachine = async () => {
    try {
      await apiFetch("/restart", { method: "POST" });
      toast.success("Restart initiated", {
        description: "Machine is restarting...",
      });
    } catch (error: unknown) {
      console.log("Machine restarting...", error);
      toast.success("Restart initiated", {
        description: "Machine is restarting...",
      });
    }
  };

  const confirmRestart = () => {
    toast("Confirm Restart", {
      description: "Are you sure you want to restart your machine?",
      action: {
        label: "Restart",
        onClick: () => restartMachine(),
      },
    });
  };

  const confirmFactoryReset = async () => {
    toast("Confirm Factory Reset", {
      description:
        "This will reset the config to defaults and restart the ESP. This can't be undone.",
      action: {
        label: "Factory Reset",
        onClick: async () => {
          try {
            await apiFetch("/factory-reset", { method: "POST" });
            toast.success("Factory reset completed", {
              description: "Machine is restarting with default settings...",
            });
          } catch (error: unknown) {
            console.log("Machine restarting after factory reset...", error);
            toast.success("Factory reset completed", {
              description: "Machine is restarting with default settings...",
            });
          }
        },
      },
    });
  };

  const confirmWifiReset = async () => {
    toast("Confirm WiFi Reset", {
      description:
        "This will erase saved WiFi credentials and restart the device.",
      action: {
        label: "Reset WiFi",
        onClick: async () => {
          try {
            const response = await apiFetch("/wifi-reset", { method: "POST" });
            const text = await response.text();
            toast.success("WiFi reset completed", {
              description: text,
            });
          } catch (error: unknown) {
            toast.error("Reset failed", {
              description: (error as Error).message,
            });
          }
        },
      },
    });
  };

  const handleFileSelect = (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files ? event.target.files[0] : null;
    setSelectedFile(file);
    setUploadMessage("");

    if (file) {
      if (!file.name.toLowerCase().endsWith(".json")) {
        setUploadMessage("Please select a valid JSON configuration file.");
        setUploadSuccess(false);
        setSelectedFile(null);
        toast.error("Invalid file type", {
          description: "Please select a valid JSON configuration file.",
        });
        return;
      }

      const maxSize = 50 * 1024; // 50KB
      if (file.size > maxSize) {
        setUploadMessage(
          "Configuration file is too large. Maximum size is 50KB."
        );
        setUploadSuccess(false);
        setSelectedFile(null);
        toast.error("File too large", {
          description: "Configuration file is too large. Maximum size is 50KB.",
        });
        return;
      }

      setUploadMessage(`Selected: ${file.name} (${formatFileSize(file.size)})`);
      setUploadSuccess(true);
      toast.success("File selected", {
        description: `Selected: ${file.name} (${formatFileSize(file.size)})`,
      });
    }
  };

  const uploadConfig = async () => {
    if (!selectedFile) {
      setUploadMessage("Please select a configuration file first.");
      setUploadSuccess(false);
      toast.error("No file selected", {
        description: "Please select a configuration file first.",
      });
      return;
    }

    setIsUploading(true);
    setUploadMessage("Uploading configuration...");

    try {
      const formData = new FormData();
      formData.append("config", selectedFile);

      const response = await apiFetch("/config/upload", {
        method: "POST",
        body: formData,
      });

      if (!response.ok) {
        let errorMessage = "Upload failed. Please try again.";
        try {
          const errorData = await response.json();
          if (errorData.message) {
            errorMessage = errorData.message;
          }
        } catch (error: unknown) {
          errorMessage = `Upload failed: ${response.status} ${response.statusText}`;
          console.error(error);
        }
        setUploadMessage(errorMessage);
        setUploadSuccess(false);
        toast.error("Upload failed", {
          description: errorMessage,
        });
        return;
      }

      let result;
      try {
        result = await response.json();
      } catch (error: unknown) {
        setUploadMessage("Configuration uploaded successfully!");
        setUploadSuccess(true);
        toast.success("Upload successful", {
          description:
            "Configuration uploaded successfully! Machine will restart...",
        });
        console.log(error);
        // Trigger restart after successful upload
        setTimeout(() => restartMachine(), 2000);
        return;
      }

      setUploadSuccess(result.success);
      const message =
        result.message ||
        (result.success
          ? "Configuration uploaded successfully!"
          : "Configuration validation failed.");
      setUploadMessage(message);

      if (result.success) {
        toast.success("Upload successful", {
          description: message + " Machine will restart...",
        });
        // Trigger restart after successful upload
        setTimeout(() => restartMachine(), 2000);
      } else {
        toast.error("Upload failed", {
          description: message,
        });
      }
    } catch (error: unknown) {
      console.error("Upload error:", error);
      let errorMessage =
        "Upload failed due to an unexpected error. Please try again.";

      if (
        (error as Error).name === "TypeError" &&
        (error as Error).message.includes("fetch")
      ) {
        errorMessage =
          "Network error: Could not connect to device. Please try again.";
      } else if ((error as Error).name === "AbortError") {
        errorMessage = "Upload was cancelled or timed out. Please try again.";
      }

      setUploadMessage(errorMessage);
      setUploadSuccess(false);
      toast.error("Upload failed", {
        description: errorMessage,
      });
    } finally {
      setIsUploading(false);
    }
  };

  return (
    <div className="container mx-auto p-6 space-y-8 max-w-7xl">
      {/* Connection Error Alert */}
      {connectionError && (
        <Alert className="border-destructive/50 text-destructive dark:border-destructive [&>svg]:text-destructive">
          <AlertCircle className="h-4 w-4" />
          <AlertDescription className="flex items-center justify-between">
            <span>{connectionError}</span>
            <Button variant="outline" size="sm" className="ml-3">
              <RefreshCw className="h-4 w-4 mr-1" />
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      )}

      {/* System Control Section */}
      <div className="space-y-6">
        <div className="flex items-center gap-3 pb-2 border-b">
          <Power className="h-6 w-6 text-orange-600" />
          <h2 className="text-2xl font-semibold">System Control</h2>
        </div>
        <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
          {/* Machine Actions */}
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-3">
                <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-indigo-500/10">
                  <Zap className="h-5 w-5 text-indigo-600" />
                </div>
                Machine Actions
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <p className="text-muted-foreground">
                Wake or put the machine into standby mode.
              </p>
              <div className="flex flex-wrap gap-2">
                <Button
                  onClick={async () => {
                    const success = await wakeFromStandby();
                    if (success) {
                      toast.success("Machine woken up", {
                        description: "Standby timer reset",
                      });
                    } else {
                      toast.error("Failed to wake machine", {
                        description: "Could not connect to the machine",
                      });
                    }
                  }}
                  className="bg-green-500 hover:bg-green-600 text-white"
                >
                  <Power className="mr-2 h-4 w-4" />
                  Wake Up
                </Button>
                <Button
                  onClick={async () => {
                    const success = await sleepFromStandby();
                    if (success) {
                      toast.success("Machine entering standby", {
                        description: "Machine is going to sleep",
                      });
                    } else {
                      toast.error("Failed to sleep machine", {
                        description: "Could not connect to the machine",
                      });
                    }
                  }}
                  variant="outline"
                >
                  <Moon className="mr-2 h-4 w-4" />
                  Sleep
                </Button>
              </div>
            </CardContent>
          </Card>

          {/* Restart Machine */}
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-3">
                <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-orange-500/10">
                  <Power className="h-5 w-5 text-orange-600" />
                </div>
                Restart Machine
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <p className="text-muted-foreground">
                Restart the machine to apply changes or resolve issues.
              </p>
              <Button
                onClick={confirmRestart}
                className="bg-orange-500 hover:bg-orange-600 text-white"
              >
                <Power className="mr-2 h-4 w-4" />
                Restart Machine
              </Button>
            </CardContent>
          </Card>

          {/* Reset Options */}
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-3">
                <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-red-500/10">
                  <TriangleAlert className="h-5 w-5 text-red-600" />
                </div>
                Reset Options
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <p className="text-muted-foreground">
                <strong>Warning:</strong> These actions cannot be undone.
              </p>
              <div className="flex flex-wrap gap-2">
                <Button onClick={confirmFactoryReset} variant="destructive">
                  <TriangleAlert className="mr-2 h-4 w-4" />
                  Factory Reset
                </Button>
                <Button
                  onClick={confirmWifiReset}
                  className="bg-orange-500 hover:bg-orange-600 text-white"
                >
                  <Wifi className="mr-2 h-4 w-4" />
                  Reset WiFi
                </Button>
              </div>
            </CardContent>
          </Card>
        </div>
      </div>

      {/* Configuration Management Section */}
      <div className="space-y-6">
        <div className="flex items-center gap-3 pb-2 border-b">
          <Download className="h-6 w-6 text-blue-600" />
          <h2 className="text-2xl font-semibold">Configuration Management</h2>
        </div>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
          {/* Configuration Backup */}
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-3">
                <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-green-500/10">
                  <Download className="h-5 w-5 text-green-600" />
                </div>
                Configuration Backup
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <p className="text-muted-foreground">
                Download your current configuration as a backup file.
              </p>
              <a
                href={`${API_BASE_URL}/config/download`}
                download="clevercoffee-config.json"
              >
                <Button className="bg-green-500 hover:bg-green-600 text-white">
                  <Download className="mr-2 h-4 w-4" />
                  Download Config
                </Button>
              </a>
            </CardContent>
          </Card>

          {/* Configuration Upload */}
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-3">
                <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-blue-500/10">
                  <Upload className="h-5 w-5 text-blue-600" />
                </div>
                Configuration Upload
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <p className="text-muted-foreground">
                Upload a previously downloaded configuration file to restore
                settings.
              </p>
              <div className="space-y-3">
                <Input
                  type="file"
                  id="configFileInput"
                  accept=".json"
                  onChange={handleFileSelect}
                />
                <Button
                  onClick={uploadConfig}
                  disabled={!selectedFile || isUploading}
                  className="bg-blue-500 hover:bg-blue-600 text-white w-full"
                >
                  {isUploading ? (
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  ) : (
                    <Upload className="mr-2 h-4 w-4" />
                  )}
                  {isUploading ? "Uploading..." : "Upload Config"}
                </Button>
              </div>
              {uploadMessage && (
                <Alert
                  className={
                    uploadSuccess
                      ? "border-green-500 bg-green-50"
                      : "border-red-500 bg-red-50"
                  }
                >
                  <AlertDescription>{uploadMessage}</AlertDescription>
                </Alert>
              )}
            </CardContent>
          </Card>
        </div>
      </div>

      <OTAUpdateSection />
    </div>
  );
}
