import { NextRequest, NextResponse } from 'next/server';

const backendBaseUrl = process.env.MERIDIAN_BACKEND_URL ?? 'http://127.0.0.1:3001';

type RouteContext = {
  params: Promise<{ path?: string[] }>;
};

const proxy = async (request: NextRequest, context: RouteContext) => {
  const { path = [] } = await context.params;
  const url = new URL(request.url);
  const backendUrl = new URL(`/api/${path.join('/')}${url.search}`, backendBaseUrl);
  const body = request.method === 'GET' || request.method === 'HEAD' ? undefined : await request.text();

  try {
    const response = await fetch(backendUrl, {
      method: request.method,
      headers: { 'content-type': request.headers.get('content-type') ?? 'application/json' },
      body,
      cache: 'no-store',
    });

    const responseBody = await response.text();
    return new NextResponse(responseBody, {
      status: response.status,
      headers: { 'content-type': response.headers.get('content-type') ?? 'application/json' },
    });
  } catch (error) {
    // Log the real cause server-side only. Never expose the backend origin or
    // raw fetch error (which can embed the internal URL) to the client.
    console.error(`[meridian proxy] ${path.join('/')} failed:`, error);
    return NextResponse.json({
      success: false,
      command: path.join('/'),
      error: 'Meridian backend unavailable',
    }, { status: 502 });
  }
};

export const GET = proxy;
export const POST = proxy;
