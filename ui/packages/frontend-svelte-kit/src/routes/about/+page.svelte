<script lang="ts">
	import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card';
	import { Globe, Github, MessageCircle, Info } from 'lucide-svelte';

	const version = '1.0.0'; // Placeholder for now
</script>

<div class="container mx-auto max-w-7xl space-y-6 p-6">
	<Card>
		<CardHeader>
			<CardTitle class="flex items-center gap-3">
				<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-blue-500/10">
					<Info class="h-5 w-5 text-blue-600" />
				</div>
				Version Information
			</CardTitle>
		</CardHeader>
		<CardContent>
			<div class="space-y-2">
				<div class="flex items-center justify-between">
					<span class="text-muted-foreground">CleverCoffee Version</span>
					<span class="rounded bg-muted px-2 py-1 font-mono text-sm">{version}</span>
				</div>
				<div class="flex items-center justify-between">
					<span class="text-muted-foreground">UI Version</span>
					<span class="rounded bg-muted px-2 py-1 font-mono text-sm">2.0.0-beta</span>
				</div>
			</div>
		</CardContent>
	</Card>

	<Card>
		<CardHeader>
			<CardTitle class="flex items-center gap-3">
				<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-green-500/10">
					<MessageCircle class="h-5 w-5 text-green-600" />
				</div>
				Community & Resources
			</CardTitle>
		</CardHeader>
		<CardContent>
			<div class="grid grid-cols-1 gap-4 md:grid-cols-3">
				<a href="https://clevercoffee.de/" class="block" target="_blank" rel="noopener noreferrer">
					<div
						class="flex h-32 flex-col items-center justify-center rounded-lg border bg-card p-6 transition-colors duration-200 hover:bg-accent/50"
					>
						<Globe class="mb-3 h-8 w-8 text-blue-600" />
						<p class="text-center font-medium">Project Website</p>
						<p class="mt-1 text-center text-xs text-muted-foreground">clevercoffee.de</p>
					</div>
				</a>
				<a
					href="https://github.com/rancilio-pid/clevercoffee"
					class="block"
					target="_blank"
					rel="noopener noreferrer"
				>
					<div
						class="flex h-32 flex-col items-center justify-center rounded-lg border bg-card p-6 transition-colors duration-200 hover:bg-accent/50"
					>
						<Github class="mb-3 h-8 w-8 text-gray-700 dark:text-gray-300" />
						<p class="text-center font-medium">GitHub Repository</p>
						<p class="mt-1 text-center text-xs text-muted-foreground">Source code & issues</p>
					</div>
				</a>
				<a
					href="https://discord.gg/Kq5RFznuU4"
					class="block"
					target="_blank"
					rel="noopener noreferrer"
				>
					<div
						class="flex h-32 flex-col items-center justify-center rounded-lg border bg-card p-6 transition-colors duration-200 hover:bg-accent/50"
					>
						<MessageCircle class="mb-3 h-8 w-8 text-purple-600" />
						<p class="text-center font-medium">Discord Community</p>
						<p class="mt-1 text-center text-xs text-muted-foreground">Get help & support</p>
					</div>
				</a>
			</div>
		</CardContent>
	</Card>

	<Card>
		<CardHeader>
			<CardTitle class="flex items-center gap-3">
				<div class="flex h-10 w-10 items-center justify-center rounded-lg bg-orange-500/10">
					<Info class="h-5 w-5 text-orange-600" />
				</div>
				About CleverCoffee
			</CardTitle>
		</CardHeader>
		<CardContent class="space-y-4">
			<p class="text-muted-foreground">
				CleverCoffee is an open-source PID controller firmware for espresso machines, providing
				precise temperature control and advanced brewing features. This modern web interface offers
				real-time monitoring, parameter configuration, and system management.
			</p>
			<div class="grid grid-cols-1 gap-4 pt-4 md:grid-cols-2">
				<div class="space-y-2">
					<h4 class="font-medium">Key Features</h4>
					<ul class="space-y-1 text-sm text-muted-foreground">
						<li>• PID temperature control</li>
						<li>• Real-time monitoring</li>
						<li>• Parameter management</li>
						<li>• System configuration</li>
					</ul>
				</div>
				<div class="space-y-2">
					<h4 class="font-medium">Support</h4>
					<ul class="space-y-1 text-sm text-muted-foreground">
						<li>• Community Discord server</li>
						<li>• GitHub issues & discussions</li>
						<li>• Project documentation</li>
						<li>• Open source contributions</li>
					</ul>
				</div>
			</div>
		</CardContent>
	</Card>
</div>
