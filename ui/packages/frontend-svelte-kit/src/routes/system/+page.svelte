<script lang="ts">
  import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Alert, AlertDescription } from '$lib/components/ui/alert';
  import {
    Power,
    TriangleAlert,
    Wifi,
    Download,
    Upload,
    Loader2,
    AlertCircle,
    RefreshCw,
  } from 'lucide-svelte';
  import { toast } from 'svelte-sonner';
  import { apiFetch, API_BASE_URL } from '$lib/api-config';
  import { cleverCoffeeStore } from '$lib/stores/clever-coffee-store.svelte';
  import { get } from 'svelte/store';

  let selectedFile: File | null = null;
  let isUploading = false;
  let uploadMessage = '';
  let uploadSuccess = false;

  // Use the centralized hook for connection error handling
  let connectionError = get(cleverCoffeeStore.connectionError);

  cleverCoffeeStore.connectionError.subscribe((value) => {
    connectionError = value;
  });

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
    toast.info("Confirm Restart", {
      description: "Are you sure you want to restart your machine?",
      action: {
        label: "Restart",
        onClick: () => restartMachine(),
      },
    });
  };

  const confirmFactoryReset = async () => {
    toast.info("Confirm Factory Reset", {
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
    toast.info("Confirm WiFi Reset", {
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

  const handleFileSelect = (event: Event) => {
    const input = event.target as HTMLInputElement;
    const file = input.files ? input.files[0] : null;
    selectedFile = file;
    uploadMessage = "";

    if (file) {
      if (!file.name.toLowerCase().endsWith(".json")) {
        uploadMessage = "Please select a valid JSON configuration file.";
        uploadSuccess = false;
        selectedFile = null;
        toast.error("Invalid file type", {
          description: "Please select a valid JSON configuration file.",
        });
        return;
      }

      const maxSize = 50 * 1024; // 50KB
      if (file.size > maxSize) {
        uploadMessage =
          "Configuration file is too large. Maximum size is 50KB.";
        uploadSuccess = false;
        selectedFile = null;
        toast.error("File too large", {
          description: "Configuration file is too large. Maximum size is 50KB.",
        });
        return;
      }

      uploadMessage = `Selected: ${file.name} (${formatFileSize(file.size)})`;
      uploadSuccess = true;
      toast.success("File selected", {
        description: `Selected: ${file.name} (${formatFileSize(file.size)})`,
      });
    }
  };

  const uploadConfig = async () => {
    if (!selectedFile) {
      uploadMessage = "Please select a configuration file first.";
      uploadSuccess = false;
      toast.error("No file selected", {
        description: "Please select a configuration file first.",
      });
      return;
    }

    isUploading = true;
    uploadMessage = "Uploading configuration...";

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
        uploadMessage = errorMessage;
        uploadSuccess = false;
        toast.error("Upload failed", {
          description: errorMessage,
        });
        return;
      }

      let result;
      try {
        result = await response.json();
      } catch (error: unknown) {
        uploadMessage = "Configuration uploaded successfully!";
        uploadSuccess = true;
        toast.success("Upload successful", {
          description:
            "Configuration uploaded successfully! Machine will restart...",
        });
        console.log(error);
        // Trigger restart after successful upload
        setTimeout(() => restartMachine(), 2000);
        return;
      }

      uploadSuccess = result.success;
      const message =
        result.message ||
        (result.success
          ? "Configuration uploaded successfully!"
          : "Configuration validation failed.");
      uploadMessage = message;

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

      uploadMessage = errorMessage;
      uploadSuccess = false;
      toast.error("Upload failed", {
        description: errorMessage,
      });
    } finally {
      isUploading = false;
    }
  };
</script>

<div class="container mx-auto p-6 space-y-6 max-w-7xl">
  <!-- Connection Error Alert -->
  {#if connectionError}
    <Alert class="border-destructive/50 text-destructive dark:border-destructive [&>svg]:text-destructive">
      <AlertCircle class="h-4 w-4" />
      <AlertDescription class="flex items-center justify-between">
        <span>{connectionError}</span>
        <Button variant="outline" size="sm" class="ml-3" on:click={() => cleverCoffeeStore.retryConnection()}>
          <RefreshCw class="h-4 w-4 mr-1" />
          Retry
        </Button>
      </AlertDescription>
    </Alert>
  {/if}

  <!-- System Control Cards -->
  <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
    <!-- Restart Machine -->
    <Card>
      <CardHeader>
        <CardTitle class="flex items-center gap-3">
          <div class="flex h-10 w-10 items-center justify-center rounded-lg bg-orange-500/10">
            <Power class="h-5 w-5 text-orange-600" />
          </div>
          Restart Machine
        </CardTitle>
      </CardHeader>
      <CardContent class="space-y-4">
        <p class="text-muted-foreground">
          Restart the machine to apply changes or resolve issues.
        </p>
        <Button
          on:click={confirmRestart}
          class="bg-orange-500 hover:bg-orange-600 text-white"
        >
          <Power class="mr-2 h-4 w-4" />
          Restart Machine
        </Button>
      </CardContent>
    </Card>

    <!-- Reset Options -->
    <Card>
      <CardHeader>
        <CardTitle class="flex items-center gap-3">
          <div class="flex h-10 w-10 items-center justify-center rounded-lg bg-red-500/10">
            <TriangleAlert class="h-5 w-5 text-red-600" />
          </div>
          Reset Options
        </CardTitle>
      </CardHeader>
      <CardContent class="space-y-4">
        <p class="text-muted-foreground">
          <strong>Warning:</strong> These actions cannot be undone.
        </p>
        <div class="flex flex-wrap gap-2">
          <Button on:click={confirmFactoryReset} variant="destructive">
            <TriangleAlert class="mr-2 h-4 w-4" />
            Factory Reset
          </Button>
          <Button
            on:click={confirmWifiReset}
            class="bg-orange-500 hover:bg-orange-600 text-white"
          >
            <Wifi class="mr-2 h-4 w-4" />
            Reset WiFi
          </Button>
        </div>
      </CardContent>
    </Card>

    <!-- Configuration Backup -->
    <Card>
      <CardHeader>
        <CardTitle class="flex items-center gap-3">
          <div class="flex h-10 w-10 items-center justify-center rounded-lg bg-green-500/10">
            <Download class="h-5 w-5 text-green-600" />
          </div>
          Configuration Backup
        </CardTitle>
      </CardHeader>
      <CardContent class="space-y-4">
        <p class="text-muted-foreground">
          Download your current configuration as a backup file.
        </p>
        <a
          href={`${API_BASE_URL}/config/download`}
          download="clevercoffee-config.json"
        >
          <Button class="bg-green-500 hover:bg-green-600 text-white">
            <Download class="mr-2 h-4 w-4" />
            Download Config
          </Button>
        </a>
      </CardContent>
    </Card>

    <!-- Configuration Upload -->
    <Card>
      <CardHeader>
        <CardTitle class="flex items-center gap-3">
          <div class="flex h-10 w-10 items-center justify-center rounded-lg bg-blue-500/10">
            <Upload class="h-5 w-5 text-blue-600" />
          </div>
          Configuration Upload
        </CardTitle>
      </CardHeader>
      <CardContent class="space-y-4">
        <p class="text-muted-foreground">
          Upload a previously downloaded configuration file to restore
          settings.
        </p>
        <div class="space-y-3">
          <Input
            type="file"
            id="configFileInput"
            accept=".json"
            on:change={handleFileSelect}
          />
          <Button
            on:click={uploadConfig}
            disabled={!selectedFile || isUploading}
            class="bg-blue-500 hover:bg-blue-600 text-white w-full"
          >
            {#if isUploading}
              <Loader2 class="mr-2 h-4 w-4 animate-spin" />
            {:else}
              <Upload class="mr-2 h-4 w-4" />
            {/if}
            {#if isUploading}
              Uploading...
            {:else}
              Upload Config
            {/if}
          </Button>
        </div>
        {#if uploadMessage}
          <Alert
            class={
              uploadSuccess
                ? "border-green-500 bg-green-50"
                : "border-red-500 bg-red-50"
            }
          >
            <AlertDescription>{uploadMessage}</AlertDescription>
          </Alert>
        {/if}
      </CardContent>
    </Card>
  </div>
</div>
