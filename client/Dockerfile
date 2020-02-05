FROM node:12.14 as build
WORKDIR /usr/src/app
COPY package.json yarn.lock ./
RUN yarn install
RUN yarn build --env=production

FROM nginx:1.17-alpine
COPY --from=build /usr/src/app/build /usr/share/nginx/html
COPY nginx.conf /etc/nginx/nginx.conf
EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]
