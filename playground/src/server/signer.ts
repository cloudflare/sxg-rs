/**
 * Copyright 2022 Google LLC
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     https://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

export type Signer = (message: Uint8Array) => Promise<Uint8Array>;

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function fromJwk(subtle: any, jwk: Object): Signer {
  const privateKeyPromise = (async function initPrivateKey() {
    return await subtle.importKey(
      'jwk',
      jwk,
      {
        name: 'ECDSA',
        namedCurve: 'P-256',
      },
      /*extractable=*/ false,
      ['sign']
    );
  })();
  return async function signer(message: Uint8Array): Promise<Uint8Array> {
    const privateKey = await privateKeyPromise;
    const signature = await subtle.sign(
      {
        name: 'ECDSA',
        hash: 'SHA-256',
      },
      privateKey,
      message
    );
    return new Uint8Array(signature);
  };
}
