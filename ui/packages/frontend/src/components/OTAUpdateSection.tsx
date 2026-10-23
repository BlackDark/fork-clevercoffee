import { CheckCircle, HardDrive, Loader2, Upload, XCircle } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { apiFetch } from "@/lib/api-config";

// The web UI is part of the firmware image: a firmware upload updates both.
const MAX_FIRMWARE_SIZE = 2 * 1024 * 1024;

export function OTAUpdateSection() {
  const [otaProgress, setOtaProgress] = useState(0);
  const [otaStatus, setOtaStatus] = useState("idle");
  const [otaMessage, setOtaMessage] = useState("");
  const [selectedOtaFile, setSelectedOtaFile] = useState<File | null>(null);
  const [isOtaUploading, setIsOtaUploading] = useState(false);

  const formatFileSize = (bytes: number) => {
    if (bytes === 0) return "0 Bytes";
    const k = 1024;
    const sizes = ["Bytes", "KB", "MB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${parseFloat((bytes / k ** i).toFixed(2))} ${sizes[i]}`;
  };

  const resetOtaStatus = () => {
    setOtaProgress(0);
    setOtaStatus("idle");
    setOtaMessage("");
  };

  const pollOtaStatus = async () => {
    try {
      const response = await fetch("/api/ota/status");
      if (!response.ok) return null;
      const status = await response.json();
      setOtaProgress(status.progress ?? 0);
      setOtaStatus(status.status ?? "idle");
      setOtaMessage(status.error || status.message || "");
      return status;
    } catch {
      return null;
    }
  };

  const handleOtaFileSelect = (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files ? event.target.files[0] : null;
    setSelectedOtaFile(file);
    if (!file) return;

    if (!file.name.toLowerCase().endsWith(".bin")) {
      toast.error("Invalid file type", {
        description: "Please select a .bin firmware file.",
      });
      setSelectedOtaFile(null);
      return;
    }
    if (file.size > MAX_FIRMWARE_SIZE) {
      toast.error("File too large", {
        description: `Maximum size is ${MAX_FIRMWARE_SIZE / 1024 / 1024}MB.`,
      });
      setSelectedOtaFile(null);
      return;
    }
    toast.success("Firmware file selected", {
      description: `Selected: ${file.name} (${formatFileSize(file.size)})`,
    });
  };

  const uploadOtaFile = async () => {
    if (!selectedOtaFile) {
      toast.error("No file selected", {
        description: "Please select a firmware file first.",
      });
      return;
    }

    toast("Confirm Firmware Update", {
      description:
        "This will update the device firmware. The device will restart automatically. Are you sure?",
      action: {
        label: "Update Firmware",
        onClick: async () => {
          const currentStatus = await pollOtaStatus();
          if (currentStatus?.updateInProgress) {
            toast.warning("Update already in progress", {
              description: "Please wait for the current update to complete.",
            });
            return;
          }

          resetOtaStatus();
          setIsOtaUploading(true);

          const pollInterval = setInterval(async () => {
            const status = await pollOtaStatus();
            if (
              status &&
              (status.status === "complete" || status.status === "error")
            ) {
              clearInterval(pollInterval);
            }
          }, 1000);

          try {
            const formData = new FormData();
            formData.append("firmware", selectedOtaFile);

            const response = await apiFetch("/ota/firmware", {
              method: "POST",
              body: formData,
            });
            const result = await response.json();

            if (result.success) {
              toast.success("Firmware update successful", {
                description: "Device will restart with the new firmware...",
              });
              setSelectedOtaFile(null);
            } else {
              const errorMessage = result.message || "Unknown error occurred";
              if (response.status === 409) {
                toast.warning("Update already in progress", {
                  description: errorMessage,
                });
              } else {
                toast.error("Firmware update failed", {
                  description: errorMessage,
                });
              }
            }
          } catch (error: unknown) {
            console.error("firmware upload error:", error);
            toast.error("Firmware update failed", {
              description: "Network error or device restarting...",
            });
          } finally {
            clearInterval(pollInterval);
            setIsOtaUploading(false);
            if (otaStatus !== "complete") {
              setTimeout(() => {
                resetOtaStatus();
              }, 5000);
            }
          }
        },
      },
    });
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-3 pb-2 border-b">
        <HardDrive className="h-6 w-6 text-purple-600" />
        <h2 className="text-2xl font-semibold">OTA Updates</h2>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-3">
              <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-purple-500/10">
                <HardDrive className="h-5 w-5 text-purple-600" />
              </div>
              Firmware Update
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="space-y-3">
              <Label htmlFor="otaFileInput">Select Firmware File</Label>
              <Input
                type="file"
                id="otaFileInput"
                accept=".bin"
                onChange={handleOtaFileSelect}
              />
            </div>
            <p className="text-muted-foreground">
              Upload a firmware file (it includes this web interface). The
              device will restart automatically after a successful update.
            </p>

            <Button
              onClick={uploadOtaFile}
              disabled={!selectedOtaFile || isOtaUploading}
              className="bg-purple-500 hover:bg-purple-600 text-white w-full"
            >
              {isOtaUploading ? (
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
              ) : (
                <Upload className="mr-2 h-4 w-4" />
              )}
              {isOtaUploading ? "Updating..." : "Upload Firmware"}
            </Button>

            {selectedOtaFile && !isOtaUploading && (
              <Alert className="border-purple-500 bg-purple-50">
                <AlertDescription>
                  Selected: {selectedOtaFile.name} (
                  {formatFileSize(selectedOtaFile.size)})
                </AlertDescription>
              </Alert>
            )}

            {(isOtaUploading || otaStatus !== "idle") && (
              <div className="space-y-3">
                <Alert
                  className={`${
                    otaStatus === "error"
                      ? "border-red-500 bg-red-50"
                      : otaStatus === "complete"
                        ? "border-green-500 bg-green-50"
                        : "border-blue-500 bg-blue-50"
                  }`}
                >
                  <div className="flex items-center gap-2">
                    {otaStatus === "error" && (
                      <XCircle className="h-4 w-4 text-red-600" />
                    )}
                    {otaStatus === "complete" && (
                      <CheckCircle className="h-4 w-4 text-green-600" />
                    )}
                    {(otaStatus === "uploading" ||
                      otaStatus === "processing") && (
                      <Loader2 className="h-4 w-4 animate-spin text-blue-600" />
                    )}
                    <AlertDescription className="flex-1">
                      {otaMessage}
                    </AlertDescription>
                  </div>
                </Alert>

                {otaProgress > 0 && (
                  <div className="w-full bg-gray-200 rounded-full h-2">
                    <div
                      className={`h-2 rounded-full transition-all duration-300 ${
                        otaStatus === "error"
                          ? "bg-red-500"
                          : otaStatus === "complete"
                            ? "bg-green-500"
                            : "bg-blue-500"
                      }`}
                      style={{ width: `${otaProgress}%` }}
                    ></div>
                  </div>
                )}
              </div>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
