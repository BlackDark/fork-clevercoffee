<script lang="ts">
	import { onMount } from 'svelte';
	import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card';
	import { Button } from '$lib/components/ui/button';
	import { AlertCircle, Home, ArrowLeft } from 'lucide-svelte';
	import { page } from '$app/stores';

	onMount(() => {
		document.title = '404 - Page Not Found | CleverCoffee';
		const metaRobots = document.createElement('meta');
		metaRobots.name = 'robots';
		metaRobots.content = 'noindex, nofollow';
		document.head.appendChild(metaRobots);

		console.warn(`404 - Page not found: ${$page.url.pathname}`);

		return () => {
			document.title = 'CleverCoffee';
			document.head.removeChild(metaRobots);
		};
	});
</script>

<div class="container mx-auto max-w-7xl space-y-6 p-6">
	<div class="flex min-h-[60vh] items-center justify-center">
		<Card class="w-full max-w-lg">
			<CardHeader class="text-center">
				<div
					class="mx-auto mb-4 flex h-16 w-16 items-center justify-center rounded-lg bg-red-500/10"
				>
					<AlertCircle class="h-8 w-8 text-red-600" />
				</div>
				<CardTitle class="text-3xl font-bold">404</CardTitle>
				<p class="text-muted-foreground text-lg">Page Not Found</p>
			</CardHeader>
			<CardContent class="space-y-6 text-center">
				<div class="space-y-2">
					<h2 class="text-xl font-semibold">Oops! This page doesn't exist</h2>
					<p class="text-muted-foreground">
						The page you're looking for might have been moved, deleted, or you may have entered an
						incorrect URL.
					</p>
				</div>

				<div class="flex flex-col justify-center gap-3 sm:flex-row">
					<Button>
						<a href="/">
							<Home class="mr-2 h-4 w-4" />
							Go to Home
						</a>
					</Button>
					<Button variant="outline" onclick={() => window.history.back()}>
						<ArrowLeft class="mr-2 h-4 w-4" />
						Go Back
					</Button>
				</div>

				<div class="border-t pt-4">
					<p class="text-muted-foreground mb-3 text-sm">Available pages:</p>
					<div class="flex flex-wrap justify-center gap-2">
						<a href="/" class="text-primary text-sm hover:underline">Home</a>
						<span class="text-muted-foreground">•</span>
						<a href="/config/behavior" class="text-primary text-sm hover:underline">Parameters</a>
						<span class="text-muted-foreground">•</span>
						<a href="/system" class="text-primary text-sm hover:underline">System</a>
						<span class="text-muted-foreground">•</span>
						<a href="/about" class="text-primary text-sm hover:underline">About</a>
					</div>
				</div>
			</CardContent>
		</Card>
	</div>
</div>
